"""Read-only library import, selection parity, shell quoting and simulator launch."""
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('game_library', ROOT/'deploy/game-library.py')
g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)

class GameLibraryTest(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='cartridge-game-test-');self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name).resolve();self.roms=self.root/'roms';(self.roms/'snes').mkdir(parents=True)
        self.rom=self.roms/'snes'/"A 'game' $(touch OWNED) %CORE%.sfc";self.rom.write_bytes(b'fixture only')
        self.config=self.root/'systems.cfg'
        self.config.write_text('''<systemList><system><name>snes</name><fullname>Super Nintendo</fullname><path>/roms/snes</path><extension>.sfc .SFC</extension><command>printf '%s' %ROM% 2>&1</command><emulators><emulator name="retroarch"><cores><core>fast</core><core>accurate</core></cores></emulator><emulator name="standalone" command="printf '%s' %ROM%"/></emulators></system><system><name>options</name><path>/opt/system</path><platform>ignore</platform></system></systemList>''')
        self.settings=self.root/'settings.cfg';self.settings.write_text('''<?xml version="1.0"?><string name="snes.emulator" value="retroarch"/><string name="snes.core" value="accurate"/><string name="GlobalPerformanceGovernor" value="powersave"/>''')
        self.lib=g.Library(self.config,self.settings,self.roms)
        self.before={p: p.read_bytes() for p in (self.rom,self.config,self.settings)}
    def tearDown(self):
        for path,data in self.before.items():self.assertEqual(path.read_bytes(),data)
    def test_stock_ampersand_settings_fragments_and_hidden_tools(self):
        self.assertEqual([s['id'] for s in self.lib.list_systems()],['snes'])
        games=self.lib.list_games('snes');self.assertEqual(len(games),1)
        self.assertEqual(games[0]['path'],str(self.rom))
    def test_settings_and_per_game_overrides(self):
        plan=self.lib.plan('snes',self.rom)
        self.assertEqual((plan['emulator'],plan['core'],plan['governor']),('retroarch','accurate','powersave'))
        import xml.etree.ElementTree as E
        root=E.Element('gameList');game=E.SubElement(root,'game')
        for k,v in {'path':'./'+self.rom.name,'name':'Chosen','emulator':'standalone','governor':'ondemand'}.items():E.SubElement(game,k).text=v
        E.ElementTree(root).write(self.rom.parent/'gamelist.xml',encoding='utf-8')
        plan=self.lib.plan('snes',self.rom)
        self.assertEqual((plan['emulator'],plan['core'],plan['governor']),('standalone','','ondemand'))
    def test_rom_filename_is_a_single_literal_argument(self):
        plan=self.lib.plan('snes',self.rom)
        result=subprocess.run(['/bin/bash','-c',plan['command']],cwd=self.root,capture_output=True,text=True,check=True)
        self.assertEqual(result.stdout,str(self.rom));self.assertFalse((self.root/'OWNED').exists())
    def test_quoted_placeholders_and_percent_tokens_in_names(self):
        value="a' \"$HOME `touch BAD` $(touch BAD2) %CORE%"
        for template in ["printf '%s' %ROM%", 'printf \'%s\' "%ROM%"', "printf '%s' '%ROM%'"]:
            command=g.expand_command(template,{'ROM':value})
            r=subprocess.run(['/bin/bash','-c',command],cwd=self.root,capture_output=True,text=True,check=True)
            self.assertEqual(r.stdout,value)
        self.assertFalse((self.root/'BAD').exists());self.assertFalse((self.root/'BAD2').exists())
    def test_refuses_outside_library_and_unknown_extension(self):
        outside=self.root/'outside.sfc';outside.touch()
        for path in [outside,self.config]:
            with self.assertRaises(ValueError):self.lib.plan('snes',path)
    def test_symlink_escape_is_not_indexed(self):
        out=self.root/'external';out.mkdir();(out/'other.sfc').touch()
        (self.rom.parent/'external').symlink_to(out,target_is_directory=True)
        self.assertEqual(len(self.lib.list_games('snes')),1)
    def test_hidden_games_and_artwork_path_boundary(self):
        (self.rom.parent/'gamelist.xml').write_text('<gameList><game><path>'+str(self.rom).replace('&','&amp;')+'</path><hidden>true</hidden></game></gameList>')
        self.assertEqual(self.lib.list_games('snes'),[])
    def test_simulator_does_not_execute_emulator(self):
        plan=self.lib.plan('snes',self.rom);plan['command']='touch SHOULD_NOT_EXIST'
        with patch.dict(os.environ,{'CARTRIDGE_SIM':'1','CARTRIDGE_HOME':str(self.root)}):result=g.launch(plan)
        self.assertTrue(result['simulated']);self.assertFalse((self.root/'SHOULD_NOT_EXIST').exists())
        self.assertTrue((self.root/'.cartridges/games/last-launch.json').is_file())
    def test_standalone_process_launch_and_failure_log(self):
        plan=self.lib.plan('snes',self.rom);plan['cwd']=str(self.root)
        plan['command']="printf 'standalone executed'; exit 7"
        with patch.dict(os.environ,{'CARTRIDGE_SIM':'0','CARTRIDGE_HOME':str(self.root)}),patch.object(g.sys,'platform','linux'):
            with self.assertRaisesRegex(RuntimeError,'status 7'):g.launch(plan)
        self.assertIn('standalone executed',(self.root/'.cartridges/games/launch.log').read_text())
    def test_unknown_placeholder_fails_closed(self):
        with self.assertRaisesRegex(ValueError,'Unsupported'):g.expand_command('x %MISSING%',{})

if __name__=='__main__':unittest.main()
