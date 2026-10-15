from noon import *

SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="12" height="20" fill="#ff0000"/><path d="M20 20 L40 20 L30 0 Z" fill="#0000ff" fill-opacity=".5"/></svg>'


class LiveSvgScene(Scene):
    def construct(self):
        self.wait(.1)
        self.svg = SVGMobject.from_string(SVG)
        self.add(self.svg)
        self.svg.shift(RIGHT)
        self.svg.set_fill("#00ff00")
        self.wait(.1)
