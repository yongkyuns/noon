// Independent raster expectations, not used by either authoring frontend.
import assert from "node:assert/strict";
export function assertLiveCoordinatePixels(png, time, regionCount) {
  const green = (r,g,b) => g > r + 25 && g > b + 25;
  const orange = (r,g,b) => r > g + 20 && g > b + 20;
  const white = (r,g,b) => r > 180 && g > 180 && b > 180;
  const blue = (r,g,b) => b > r + 35 && g > r + 20;
  assert.ok(regionCount(png,-5,2.6,green)>5, "unrelated sentinel moved or disappeared");
  if (time < 0.25) {
    assert.equal(regionCount(png,0,2,orange),0,"NumberLine appeared before construction");
    assert.equal(regionCount(png,0,0.8,white),0,"Axes appeared before construction");
    return;
  }
  assert.ok(regionCount(png,0,2,orange)>2,"late NumberLine shaft missing");
  const shift = 0.5 * Math.min(1,Math.max(0,(time-0.25)/0.5));
  assert.ok(regionCount(png,shift,0.8,white)>2,"late axes did not follow their live transform");
  if (time >= 0.75) {
    const alpha = Math.min(1,(time-0.75)/0.5);
    const point = a => [-3.5+8*a,-2+3*a];
    if (alpha>0) assert.ok(regionCount(png,...point(alpha/2),blue)>2,
      "new curve did not use the moved axes coordinate frame");
    if (alpha<1) assert.equal(regionCount(png,...point(alpha+(1-alpha)*0.75),blue),0,
      "new curve revealed future geometry early");
  }
}
