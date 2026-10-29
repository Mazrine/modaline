-- Build the demo scene used by the README screenshots.
--
--   modaline -b blank.png --script examples/build_demo.lua --save-as examples/demo.vpx
--
-- blank.png is any image; resize() makes the canvas 96x64. Everything is drawn
-- with the scripting API so the example is reproducible.

local W, H = 96, 64
local s = app.activeSprite
s:resize(W, H)

local function rgb(r, g, b, a)
  return { r = r, g = g, b = b, a = a or 255 }
end

local C = {
  night = rgb(26, 28, 44),
  indigo = rgb(41, 54, 111),
  purple = rgb(93, 39, 93),
  red = rgb(177, 62, 83),
  orange = rgb(239, 125, 87),
  gold = rgb(255, 205, 117),
  pale = rgb(244, 244, 244),
  green = rgb(56, 183, 100),
  teal = rgb(37, 113, 121),
  blue = rgb(59, 93, 201),
  sky = rgb(65, 166, 246),
  grey = rgb(148, 176, 194),
}

-- layer 1 is the initial (top) layer; newLayer() inserts below the active one.
app.activeLayer.name = "cloud"
local function add_layer(name)
  s:newLayer()
  app.activeLayer.name = name
end
add_layer("stars")
add_layer("trees")
add_layer("house")
add_layer("hills")
add_layer("sun")
add_layer("sky")

-- cloud=1, stars=2, trees=3, house=4, hills=5, sun=6, sky=7
s:newFrame()
s:newFrame()
for f = 1, 3 do
  s:setFrameDuration(f, 250)
end
s:newTag("dusk", 1, 3)

local function rect(img, x0, y0, x1, y1, c)
  for y = math.max(0, y0), math.min(H - 1, y1) do
    for x = math.max(0, x0), math.min(W - 1, x1) do
      img:setPixel(x, y, c)
    end
  end
end

local function circle(img, cx, cy, r, c)
  for y = math.max(0, cy - r), math.min(H - 1, cy + r) do
    for x = math.max(0, cx - r), math.min(W - 1, cx + r) do
      local dx, dy = x - cx, y - cy
      if dx * dx + dy * dy <= r * r then
        img:setPixel(x, y, c)
      end
    end
  end
end

local function put(layer, frame, img)
  s:setCelImage(layer, frame, img)
end

local function put_all(layer, img)
  for f = 1, 3 do
    put(layer, f, img)
  end
end

-- sky: horizontal bands, warm near the horizon
local sky = Image(W, H)
for y = 0, H - 1 do
  local c
  if y < 8 then
    c = C.night
  elseif y < 16 then
    c = C.indigo
  elseif y < 24 then
    c = C.purple
  elseif y < 31 then
    c = C.red
  elseif y < 37 then
    c = C.orange
  elseif y < 43 then
    c = C.gold
  else
    c = C.teal
  end
  rect(sky, 0, y, W - 1, y, c)
end
put_all(7, sky)

-- sun with a soft rim
local sun = Image(W, H)
circle(sun, 72, 20, 11, C.gold)
circle(sun, 72, 20, 9, C.orange)
circle(sun, 72, 20, 6, C.pale)
put_all(6, sun)

-- two hill ranges
local hills = Image(W, H)
for x = 0, W - 1 do
  local far = 40 + math.floor(4 * math.sin(x / 9.0) + 2 * math.sin(x / 4.0))
  rect(hills, x, far, x, H - 1, C.purple)
end
for x = 0, W - 1 do
  local near = 50 + math.floor(3 * math.sin(x / 13.0 + 1.0))
  rect(hills, x, near, x, H - 1, C.night)
end
put_all(5, hills)

-- a small cabin on the near hill
local house = Image(W, H)
rect(house, 20, 40, 34, 53, C.red)
rect(house, 20, 40, 34, 40, C.purple)
for i = 0, 7 do
  rect(house, 20 + i, 40 - i, 34 - i, 40 - i, C.purple)
end
rect(house, 25, 46, 29, 53, C.night)      -- door
rect(house, 31, 44, 33, 46, C.gold)       -- lit window
rect(house, 22, 44, 23, 45, C.pale)       -- small window
put_all(4, house)

-- pines on both sides
local trees = Image(W, H)
local function pine(x, base, size)
  for i = 0, size do
    local half = math.floor((size - i) / 2)
    rect(trees, x - half, base - i, x + half, base - i, C.teal)
  end
  rect(trees, x, base, x, base + 5, C.purple)
end
pine(8, 50, 16)
pine(14, 53, 11)
pine(84, 48, 18)
pine(90, 52, 12)
put_all(3, trees)

-- stars: a deterministic scatter, twinkling on frame 3
local stars = Image(W, H)
local star_pts = {
  { 10, 6 }, { 22, 11 }, { 35, 4 }, { 48, 9 }, { 58, 3 },
  { 86, 8 }, { 92, 14 }, { 5, 18 }, { 41, 17 }, { 66, 6 },
}
for i, p in ipairs(star_pts) do
  if i % 3 ~= 0 then
    stars:setPixel(p[1], p[2], C.pale)
  else
    stars:setPixel(p[1], p[2], C.grey)
  end
end
put(2, 1, stars)
local stars2 = s:cel(2, 1).image
put(2, 2, stars2)
local stars3 = s:cel(2, 2).image
for _, p in ipairs(star_pts) do
  if (p[1] + p[2]) % 2 == 0 then
    stars3:setPixel(p[1], p[2], C.gold)
  end
end
put(2, 3, stars3)

-- cloud drifts across the three frames
for f = 1, 3 do
  local cloud = Image(W, H)
  local ox = 6 + (f - 1) * 9
  circle(cloud, ox, 14, 4, C.grey)
  circle(cloud, ox + 6, 13, 5, C.grey)
  circle(cloud, ox + 12, 14, 4, C.grey)
  rect(cloud, ox - 2, 14, ox + 15, 16, C.grey)
  put(1, f, cloud)
end

print("demo scene: " .. s.width .. "x" .. s.height)
