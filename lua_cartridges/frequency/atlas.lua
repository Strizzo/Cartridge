-- A compact geographic index and pre-rendered tiles; no per-frame world scan.
local A={};A.__index=A
local W,H,X,Y=672,252,24,150
local GW,GH=72,36
local function finite(n) return type(n)=='number' and n==n and math.abs(n)<math.huge end
local function clamp(n,a,b) return math.max(a,math.min(b,n)) end
local function key(u,v) return clamp(math.floor(v*GH),0,GH-1)*GW+clamp(math.floor(u*GW),0,GW-1)+1 end
function A.new(data)
    if not data then local ok,value=pcall(require,'atlas_data');if ok then data=value end end
    local self=setmetatable({records={},grid={},u=0.5,v=0.5,zoom=1,date='',countries=0},A)
    if type(data)~='table' or data.schema~=1 or data.tile_w~=W or data.tile_h~=H or type(data.stations)~='table' then
        self.error='World atlas unavailable';return self
    end
    self.date=type(data.built_at)=='string' and data.built_at:sub(1,10) or ''
    local seen,countries={},{}
    for i=1,math.min(#data.stations,30000) do
        local r=data.stations[i]
        if type(r)=='table' and type(r[1])=='string' and r[1]~='' and #r[1]<=100 and not seen[r[1]] and
            finite(r[2]) and finite(r[3]) and math.abs(r[2])<=90 and math.abs(r[3])<=180 then
            seen[r[1]]=true
            r[6]=(r[3]+180)/360;r[7]=(90-r[2])/180
            self.records[#self.records+1]=r
            local k=key(r[6],r[7]);self.grid[k]=self.grid[k] or {};self.grid[k][#self.grid[k]+1]=r
            if type(r[4])=='string' and r[4]:match('^[A-Z][A-Z]$') then countries[r[4]]=true end
        end
    end
    for _ in pairs(countries) do self.countries=self.countries+1 end
    return self
end
function A:viewport()
    local w,h=W*self.zoom,H*self.zoom
    return {x=math.floor(clamp(self.u*w-W/2,0,w-W)),y=math.floor(clamp(self.v*h-H/2,0,h-H)),w=w,h=h}
end
function A:project(lat,lon)
    if not finite(lat) or not finite(lon) or math.abs(lat)>90 or math.abs(lon)>180 then return nil end
    local p=self:viewport();local x=X+(lon+180)/360*p.w-p.x;local y=Y+(90-lat)/180*p.h-p.y
    if x<X or x>=X+W or y<Y or y>=Y+H then return nil end
    return x,y
end
function A:cursor() return self:project(90-self.v*180,self.u*360-180) end
function A:set_cursor(lat,lon)
    if finite(lat) and finite(lon) then self.u=clamp((lon+180)/360,0,1-1e-9);self.v=clamp((90-lat)/180,0,1-1e-9) end
end
-- Pixel speed is independent of zoom. Keep fractional movement, but report only
-- a changed image pixel (cursor or viewport) so tiny inputs do not redraw/scan.
function A:pan_pixels(dx,dy)
    if not finite(dx) or not finite(dy) then return false end
    local w,h=W*self.zoom,H*self.zoom
    local x,y=math.floor(self.u*w),math.floor(self.v*h)
    self.u=clamp(self.u+dx/w,0,1-1e-9);self.v=clamp(self.v+dy/h,0,1-1e-9)
    return x~=math.floor(self.u*w) or y~=math.floor(self.v*h)
end
function A:move(direction)
    if direction=='dpad_left' then return self:pan_pixels(-24,0)
    elseif direction=='dpad_right' then return self:pan_pixels(24,0)
    elseif direction=='dpad_up' then return self:pan_pixels(0,-24)
    elseif direction=='dpad_down' then return self:pan_pixels(0,24) end
    return false
end
function A:magnify(delta)
    local old=self.zoom
    self.zoom=clamp(self.zoom*(delta>0 and 2 or 0.5),1,8)
    return self.zoom~=old
end
function A:country(code)
    if code=='' then self.zoom=1;self.u=0.5;self.v=0.5;return end
    local lat,lon={},{}
    for _,r in ipairs(self.records) do if r[4]==code then lat[#lat+1]=r[2];lon[#lon+1]=r[3] end end
    if #lat==0 then return false end
    table.sort(lat);table.sort(lon)
    self:set_cursor(lat[math.ceil(#lat/2)],lon[math.ceil(#lon/2)]);self.zoom=4
    return true
end
function A:tiles()
    if self.error then return {} end
    local p=self:viewport();local result={}
    for row=math.floor(p.y/H),math.floor((p.y+H-1)/H) do
        for col=math.floor(p.x/W),math.floor((p.x+W-1)/W) do
            local left,top=math.max(p.x,col*W),math.max(p.y,row*H)
            local right,bottom=math.min(p.x+W,(col+1)*W),math.min(p.y+H,(row+1)*H)
            result[#result+1]={path=string.format('assets/atlas/z%d-%d-%d.png',self.zoom,col,row),
                x=X+left-p.x,y=Y+top-p.y,opts={w=right-left,h=bottom-top,src_x=left-col*W,src_y=top-row*H,src_w=right-left,src_h=bottom-top}}
        end
    end
    return result
end
function A:nearest(limit)
    limit=math.max(1,math.min(limit or 100,100))
    if #self.records==0 then return {} end
    local found,radius={},32
    while true do
        found={}
        local du,dv=radius/(W*self.zoom),radius/(H*self.zoom)
        local x0,x1=clamp(math.floor((self.u-du)*GW),0,GW-1),clamp(math.floor((self.u+du)*GW),0,GW-1)
        local y0,y1=clamp(math.floor((self.v-dv)*GH),0,GH-1),clamp(math.floor((self.v+dv)*GH),0,GH-1)
        local rr=radius*radius
        for cy=y0,y1 do for cx=x0,x1 do
            for _,r in ipairs(self.grid[cy*GW+cx+1] or {}) do
                local dx,dy=(r[6]-self.u)*W*self.zoom,(r[7]-self.v)*H*self.zoom
                local distance=dx*dx+dy*dy
                if distance<=rr and (#found<limit or distance<=found[#found].distance) then
                    local at=#found+1
                    for j,c in ipairs(found) do if distance<c.distance or (distance==c.distance and r[1]<c.row[1]) then at=j;break end end
                    if at<=limit then table.insert(found,at,{row=r,distance=distance});if #found>limit then table.remove(found) end end
                end
            end
        end end
        if #found>=limit or radius>=W*self.zoom*2 then break end
        radius=radius*2
    end
    local result={}
    for _,c in ipairs(found) do
        local r=c.row
        result[#result+1]={stationuuid=r[1],geo_lat=r[2],geo_long=r[3],countrycode=r[4] or '',country='',
            name=type(r[5])=='string' and r[5] or 'Directory station',codec='DIRECTORY',atlas=true}
    end
    return result
end
return A
