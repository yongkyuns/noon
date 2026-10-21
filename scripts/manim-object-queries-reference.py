"""Pinned ordinary-Mobject own-point and query callback controls."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import numpy as np
import manim as m

POINTS = [[0., 0., 0.], [.1, 0., 0.], [.2, .3, 0.], [.3, .4, 0.], [.5, .6, 0.]]
SLICES = [[None,None,1],[None,None,-1],[0,1,1],[1,3,1],[0,0,1],[-1,None,-1]]


def observe(spec):
    root = getattr(m, spec['class'])()
    def set_count(count):
        root.apply_over_attr_arrays(lambda _: np.asarray(POINTS[:count], dtype=float).reshape((-1,3)))
    set_count(spec['ownPoints'])
    def snapshot():
        return {'points':root.get_num_points(),'length':len(root),'iteration':['self' if value is root else 'unexpected' for value in root],'split':['self' if value is root else 'unexpected' for value in root.split()]}
    before = snapshot()
    events = []
    class Operand:
        def __init__(self, phase, value): self.phase, self.value = phase, value
        def __index__(self):
            events.append({'phase':self.phase, **snapshot()})
            if self.phase == spec['phase']:
                if spec['action']=='set_points': set_count(4)
                elif spec['action']=='clear_points': set_count(0)
                elif spec['action']!='none': raise AssertionError(spec['action'])
            return self.value
    try:
        if spec['query']=='scalar':
            selected=root[Operand('index', spec['index'])]
            result='self' if selected is root else 'unexpected'
        else:
            values=[None if value is None else Operand(phase,value) for phase,value in zip(('start','stop','step'),spec['slice'])]
            selected=root[slice(*values)]
            result={'type':type(selected).__name__,'members':['self' if value is root else 'unexpected' for value in selected.submobjects]}
    except Exception as error:
        result={'exception':type(error).__name__,'message':str(error)}
    return {'spec':spec,'before':before,'events':events,'result':result,'after':snapshot()}


def main():
    assert m.__version__=='0.21.0'
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('output',type=Path);args=parser.parse_args()
    cases=[]
    for kind in ['Mobject','VMobject','Circle','Square','VectorizedPoint']:
        for count in [0,1,4,5]:
            for action in ['none','set_points','clear_points']:
                for index in [-2,-1,0,1,10]:
                    cases.append(observe({'class':kind,'ownPoints':count,'query':'scalar','phase':'index','index':index,'action':action}))
                for values in SLICES:
                    for phase in ['start','step']:
                        cases.append(observe({'class':kind,'ownPoints':count,'query':'slice','phase':phase,'slice':values,'action':action}))
    assert len(cases)==1020
    assert any(row['result']=={'exception':'IndexError','message':'list index out of range'} for row in cases)
    result={'schemaVersion':1,'manimVersion':m.__version__,'scope':'Ordinary Mobject/VMobject/concrete-shape count, split, iteration and scalar/slice callbacks via public array mapping. Child-membership mutation, raw arrays/lists and arbitrary subclass hooks are separate requirements.',
            'generatorSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            'methodSourceSha256':{name:hashlib.sha256(inspect.getsource(getattr(m.Mobject,name)).encode()).hexdigest() for name in ['__getitem__','__iter__','__len__','get_group_class','split','get_array_attrs','apply_over_attr_arrays']},'cases':cases}
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print('Observed 1020 pinned ordinary-object query and callback controls across five public classes and 0/1/4/5 own points')


if __name__=='__main__':main()
