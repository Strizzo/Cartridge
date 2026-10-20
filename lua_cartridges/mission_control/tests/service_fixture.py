"""Run the current VibeBoy HTTP API with model fixtures and a recording executor.

No production daemon, tmux process, source mutation, or network beyond loopback.
Used by the opt-in mlua integration test; requires the real VibeBoy checkout.
"""
import argparse
import asyncio
import json
import sys
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--source', required=True)
parser.add_argument('--records', required=True)
args = parser.parse_args()
sys.path.insert(0, str(Path(args.source).resolve()))
from aiohttp import web
from vibeboy.http_api import HttpApi
from vibeboy.websocket_server import VibeBoyServer
from vibeboy.models import SessionState, SessionType, SessionStatus, ResponseOption, ResponseCategory, DetectedChoice

fixture = Path(__file__).resolve().parents[3] / 'sim' / 'fixtures' / 'mission-control.json'
raw = json.loads(fixture.read_text())[0]['body']['sessions']
server = VibeBoyServer()
for sid, row in raw.items():
    state = SessionState(row['session_name'], row['pane_id'], row['pane_command'],
                         session_type=SessionType(row['session_type']), status=SessionStatus(row['status']),
                         screen_content=row['screen_content'], permission_mode=row['permission_mode'],
                         last_updated=row['last_updated'])
    state.response_options = [ResponseOption(o['text'], ResponseCategory(o['category']), o['shortcut']) for o in row['response_options']]
    state.detected_choices = [DetectedChoice(**o) for o in row['detected_choices']]
    assert state.to_dict() == row, 'Fixture drifted from the live SessionState contract'
    server._states[sid] = state

records = []
class RecordingExecutor:
    def record(self, action, state, value=''):
        records.append({'action': action, 'pane_id': state.pane_id, 'value': value})
        Path(args.records).write_text(json.dumps(records))
        return True
    def send_text(self, state, text): return self.record('send_response', state, text)
    def send_keys(self, state, keys): return self.record('send_keys', state, keys)
    def send_tab(self, state): return self.record('accept_ghost', state, 'Tab')
    def send_interrupt(self, state): return self.record('interrupt', state, 'C-c')
    def send_suspend(self, state): return self.record('suspend', state, 'C-z')
    def send_escape(self, state): return self.record('escape', state, 'Escape')
server.set_executor(RecordingExecutor())

async def main():
    api = HttpApi(server, host='127.0.0.1', port=0)
    app = web.Application()
    # Identical handlers/routes to HttpApi.start; ephemeral listener for parallel tests.
    app.router.add_get('/api/state', api._handle_get_state)
    app.router.add_post('/api/action', api._handle_post_action)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    port = site._server.sockets[0].getsockname()[1]
    print(f'http://127.0.0.1:{port}', flush=True)
    try:
        await asyncio.Future()
    finally:
        await runner.cleanup()

asyncio.run(main())
