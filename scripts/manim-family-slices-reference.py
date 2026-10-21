"""Observe which member sequence CE slicing reads around Python protocols."""
import argparse
import hashlib
import json
import inspect
from pathlib import Path
import warnings
import manim as m

POINTS = [[0.,0.,0.],[0.1,0.,0.],[0.2,0.3,0.],[0.3,0.4,0.]]
SLICES = [[0,3,1],[1,3,1],[-1,None,-1],[None,None,1],[None,None,-1],[0,0,1]]


def observe(spec):
    a,b,extra = m.Circle(0.2),m.Square(0.3),m.Square(0.4)
    root=getattr(m,spec['class'])(a,b)
    root.set_points(POINTS if spec['ownPoints'] else [])
    names={id(root):'self',id(a):'a',id(b):'b',id(extra):'extra'}
    def snapshot():
        return {'points':root.get_num_points(),'length':len(root),'children':[names[id(v)] for v in root.submobjects]}
    captured=root.submobjects
    before=snapshot();events=[]
    class Operand:
        def __init__(self,phase,value):self.phase,self.value=phase,value
        def __index__(self):
            events.append({'phase':self.phase,**snapshot()})
            if self.phase==spec['phase']:
                action=spec['action']
                if action=='remove_first':root.remove(a)
                elif action=='clear_children':root.remove(*root.submobjects)
                elif action=='append_extra':root.add(extra)
                elif action=='set_points':root.set_points(POINTS)
                elif action=='clear_points':root.set_points([])
                elif action!='none':raise AssertionError(action)
            return self.value
    parts=[None if value is None else Operand(phase,value) for phase,value in zip(('start','stop','step'),spec['slice'])]
    try:
        selected=root[slice(*parts)]
        result={'type':type(selected).__name__,'members':[names[id(v)] for v in selected.submobjects]}
    except Exception as error:
        result={'exception':type(error).__name__,'message':str(error)}
    return {'spec':spec,'before':before,'events':events,'result':result,'after':snapshot(),'sameChildList':root.submobjects is captured,'capturedListAfter':[names[id(v)] for v in captured]}


def main():
    assert m.__version__=='0.21.0'
    warnings.simplefilter('ignore',DeprecationWarning)
    cases=[observe({'class':kind,'ownPoints':count,'slice':values,'phase':phase,'action':action})
           for kind in ('Group','VGroup') for count in (0,4) for values in SLICES
           for phase in ('start','stop','step') for action in ('none','remove_first','clear_children','append_extra','set_points','clear_points')]
    assert len(cases)==432
    def row(count):
        return next(row for row in cases if row['spec']=={'class':'Group','ownPoints':count,'slice':[0,3,1],'phase':'start','action':'remove_first'})
    assert row(0)['result']=={'type':'Group','members':['b']}
    assert row(0)['sameChildList'] is True
    added=next(row for row in cases if row['spec']=={'class':'Group','ownPoints':0,'slice':[0,3,1],'phase':'start','action':'append_extra'})
    assert added['result']=={'type':'Group','members':['a','b']}
    assert added['sameChildList'] is False
    assert added['after']['children']==['a','b','extra']
    assert row(0)['after']['children']==['b']
    assert row(4)['result']=={'exception':'IndexError','message':'list index out of range'}
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output',type=Path)
    args=parser.parse_args()
    report={'schemaVersion':1,'manimVersion':m.__version__,'generatorSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            'scope':'Pinned slice branch/capture and operand protocol timing. Public semantic projection excludes raw submobjects list identity/mutation; those are separate qualification requirements.',
            'semanticProjection':['spec','before','events','result','after'],
            'methodSourceSha256':{name:hashlib.sha256(inspect.getsource(getattr(m.Mobject,name)).encode()).hexdigest() for name in ('__getitem__','__len__','get_group_class','has_points','has_no_points','split','add','remove')},
            'cases':cases}
    args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(f'Observed {len(cases)} pinned CE slice cases; in-place removal versus list-replacing add controls verified')


if __name__=='__main__':main()
