"""Small layout helpers over ordinary Noon objects, not another scene system."""
from dataclasses import dataclass
from math import cos, sin, pi
from textwrap import wrap
from uncertainty import confidence_contour
from noon import (Color, Text, MathTypst, Line, VMobject, FadeIn, FadeOut,
                  Transform, Rotate, linear)


@dataclass(frozen=True)
class Layout:
    title_y: float = 3.38
    question_y: float = 2.76
    caption_y: float = -3.38
    caption_columns: int = 82
    caption_font_size: int = 23
    caption_line_gap: float = 0.38
    caption_width: float = 12.7
    plot_left: float = -5.80
    plot_right: float = 0.30
    plot_bottom: float = -1.95
    plot_top: float = 1.82
    note_x: float = 3.58
    note_width: float = 5.2
    transition: float = 0.55
    hold: float = 5.0


LAYOUT = Layout()
INK = Color(0.94, 0.96, 1.0)
MUTED = Color(0.58, 0.65, 0.74)
GRID = Color(0.20, 0.25, 0.33)
TRUTH = Color(0.86, 0.90, 0.96)
GNSS = Color(1.0, 0.76, 0.30)
INS = Color(1.0, 0.43, 0.43)
FUSED = Color(0.27, 0.84, 0.75)
UNCERTAINTY = Color(0.50, 0.62, 1.0)


def text(words, centre=(0, 0), size=24, color=INK, max_width=12.7):
    obj = Text(words, font_size=size, color=color)
    if obj.width > max_width:
        obj.width = max_width
    obj.move_to(centre)
    return obj


def equation(source, centre, size=33, max_width=5.2, color=INK):
    """Typeset genuine Typst math once; never recompile it inside an updater."""
    obj = MathTypst(source, font_size=size, color=color)
    if obj.width > max_width:
        obj.width = max_width
    obj.move_to(centre)
    return obj


def path(points, color=INK, width=3.0):
    return VMobject(color=color).set_points_as_corners(points).set_stroke(width=width)


def arrow(start, end, color=INK, width=3.0, tip=0.15):
    """One ordinary vector path, including two line-segment arrowhead wings."""
    dx, dy = end[0]-start[0], end[1]-start[1]
    length = (dx*dx+dy*dy)**0.5
    if length == 0:
        raise ValueError('Arrow endpoints must differ')
    ux, uy = dx/length, dy/length
    left = (end[0]-tip*ux-tip*uy/2, end[1]-tip*uy+tip*ux/2)
    right = (end[0]-tip*ux+tip*uy/2, end[1]-tip*uy-tip*ux/2)
    return path([start,end,left,end,right],color,width)


class Stage:
    """Layout and a list of authored objects to retire between explanation beats."""
    def __init__(self, scene, number, title, question):
        self.scene = scene
        self.content = []
        self.caption = []
        scene.add(text(f'{number:02d} / INS + GNSS',(-4.95,LAYOUT.title_y),16,MUTED,max_width=3),
                  text(title,(1.25,LAYOUT.title_y),31,max_width=9.3),
                  text(question,(0,LAYOUT.question_y),23,MUTED),
                  Line((-6.4,2.35),(6.4,2.35),color=GRID))

    def add(self, *objects):
        self.scene.add(*objects)
        self.content.extend(objects)

    async def reveal(self, *objects, hold=LAYOUT.hold):
        self.content.extend(objects)
        await self.scene.play(*(FadeIn(o) for o in objects), run_time=LAYOUT.transition)
        if hold:
            await self.scene.wait(hold)

    async def say(self, words, hold=LAYOUT.hold):
        # Keep explanations readable instead of shrinking a long single line.
        lines = wrap(words, width=LAYOUT.caption_columns,
                     break_long_words=False, break_on_hyphens=False)
        if not 1 <= len(lines) <= 2:
            raise ValueError('Use one or two caption lines; split longer explanations into beats')
        top = LAYOUT.caption_y + (len(lines) - 1) * LAYOUT.caption_line_gap / 2
        new = [text(line, (0, top - i * LAYOUT.caption_line_gap),
                    LAYOUT.caption_font_size, max_width=LAYOUT.caption_width)
               for i, line in enumerate(lines)]
        fade_in_time = LAYOUT.transition
        if self.caption:
            # Sequential fades avoid briefly superimposing two explanations.
            fade_in_time /= 2
            await self.scene.play(*(FadeOut(o) for o in self.caption),
                                  run_time=LAYOUT.transition - fade_in_time)
        await self.scene.play(*(FadeIn(o) for o in new), run_time=fade_in_time)
        self.caption = new
        if hold:
            await self.scene.wait(hold)

    async def clear(self):
        if self.content:
            await self.scene.play(*(FadeOut(o) for o in self.content),run_time=LAYOUT.transition)
            self.content = []

    async def finish(self, takeaway):
        await self.say(takeaway,hold=8.0)


def notes(lines, start_y=1.5, gap=0.72, color=INK):
    return [text(line,(LAYOUT.note_x,start_y-i*gap),22,color,LAYOUT.note_width)
            for i,line in enumerate(lines)]


class Plot:
    """Explicit axes in data units. No autoscale, hidden clipping, or chart library."""
    def __init__(self, xlim, ylim, xlabel, ylabel, *, bounds=None, xticks=(), yticks=()):
        self.xlim,self.ylim = xlim,ylim
        self.bounds = bounds or (LAYOUT.plot_left,LAYOUT.plot_bottom,LAYOUT.plot_right,LAYOUT.plot_top)
        if xlim[1]<=xlim[0] or ylim[1]<=ylim[0]:
            raise ValueError('Plot ranges must be increasing')
        left,bottom,right,top=self.bounds
        self.objects=[Line((left,bottom),(right,bottom),color=MUTED),
                      Line((left,bottom),(left,top),color=MUTED),
                      text(xlabel,((left+right)/2,bottom-.50),19,MUTED,max_width=right-left),
                      text(ylabel,((left+right)/2,top+.31),20,MUTED,max_width=right-left)]
        for tick in xticks:
            x,y=self.point(tick,ylim[0])
            self.objects.extend([Line((x,y),(x,y-.08),color=MUTED),text(f'{tick:g}',(x,y-.23),16,MUTED)])
        for tick in yticks:
            x,y=self.point(xlim[0],tick)
            self.objects.extend([Line((x,y),(right,y),color=GRID),text(f'{tick:g}',(x-.35,y),16,MUTED,max_width=.55)])

    def point(self,x,y):
        left,bottom,right,top=self.bounds
        return (left+(x-self.xlim[0])/(self.xlim[1]-self.xlim[0])*(right-left),
                bottom+(y-self.ylim[0])/(self.ylim[1]-self.ylim[0])*(top-bottom))

    def curve(self,xy,color=FUSED,width=3):
        return path([self.point(x,y) for x,y in xy],color,width)

    def cursor(self,x):
        return Line(self.point(x,self.ylim[0]),self.point(x,self.ylim[1]),color=GNSS).set_stroke(width=1.5)

    async def move_cursor(self, scene, cursor, start, end, duration):
        """Translate unchanged line geometry; do not morph newly baked endpoints."""
        dx = self.point(end, self.ylim[0])[0] - self.point(start, self.ylim[0])[0]
        await self.scene.play(Transform(cursor, cursor.copy().shift((dx, 0))),
                         run_time=duration, rate_func=linear)


def car(centre,color=TRUTH):
    """Single path: a simple top-view vehicle; positive x is the front."""
    x,y=centre
    outline=[(-.48,-.24),(.30,-.24),(.50,0),(.30,.24),(-.48,.24),(-.48,-.24)]
    return path([(x+dx,y+dy) for dx,dy in outline],color,3)


def confidence_ellipse(centre,covariance,scale_x=1,scale_y=1,probability=.95):
    """Joint 2D confidence region, not two independent +/-sigma intervals."""
    points = confidence_contour((0, 0), covariance, probability)
    return path([(centre[0] + scale_x*x, centre[1] + scale_y*y) for x, y in points],
                UNCERTAINTY, 2.5)


def rotating_frame(centre,length=1.45,color=FUSED):
    """A symmetric leaf path: Rotate's centre is exactly the frame origin."""
    points=[(-length,0),(length,0),(length-.15,.08),(length,0),(length-.15,-.08),(length,0),
            (0,0),(0,-length),(0,length),(-.08,length-.15),(0,length),(.08,length-.15),(0,length)]
    return path(points,color,2.5).shift(centre)
