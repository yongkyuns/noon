"""Shared caller-argument and append-hook observations; no engine implementation."""
import numpy as np
PREFIX=np.array([[.123456789012345,.234567890123456,0.], [.345678901234567,.456789012345678,0.], [.567890123456789,.67890123456789,0.], [.789012345678901,.890123456789012,0.]])
SPECS=[{'id':'empty-list','values':[],'container':'list'},
       {'id':'flat-one','values':[.123456789012345],'container':'list'},
       {'id':'flat-two','values':[.123456789012345,.987654321098765],'container':'list'},
       {'id':'flat-three','values':[.123456789012345,.987654321098765,0.],'container':'list'},
       {'id':'one-xyz','values':[[.123456789012345,.987654321098765,0.]],'container':'list'},
       {'id':'one-xy','values':[[.123456789012345,.987654321098765]],'container':'list'},
       {'id':'four-xyz','values':PREFIX.tolist(),'container':'list'},
       {'id':'flat-array','values':[.123456789012345,.987654321098765,0.],'container':'array','dtype':'float64'},
       {'id':'one-xyz-array','values':[[.123456789012345,.987654321098765,0.]],'container':'array','dtype':'float64'},
       {'id':'four-xyz-array','values':PREFIX.tolist(),'container':'array','dtype':'float64'},
       {'id':'cubic-array','values':[PREFIX.tolist()],'container':'array','dtype':'float64'},
       {'id':'float32-array','values':PREFIX.tolist(),'container':'array','dtype':'float32'},
       {'id':'int32-array','values':[[1,2,0]],'container':'array','dtype':'int32'},
       {'id':'scalar-array','values':2.,'container':'array','dtype':'float64'}]
METHODS=('append_points','add_cubic_bezier_curves','add_subpath')
def arguments(spec):
 return np.array(spec['values'],dtype=spec['dtype']) if spec['container']=='array' else spec['values'].copy()
def observe_point_append(m,spec,method,prefix_rows,constructor=None):
 v=(constructor or m.VMobject)().set_points(PREFIX[:prefix_rows].copy());arg=arguments(spec);before=np.array(arg,copy=True)
 try:
  result=getattr(v,method)(arg);out={'kind':'returned','returnsSelf':result is v}
 except Exception as e:out={'kind':'raised','type':type(e).__name__,'message':str(e)}
 return {'spec':spec,'method':method,'prefixRows':prefix_rows,'output':out,'pointsXY':[[float(p[0]),float(p[1])] for p in v.get_all_points()], 'pointsXYZ':[[float(c) for c in p] for p in v.get_all_points()], 'numPoints':v.get_num_points(),'inputUnchanged':bool(np.array_equal(np.asarray(arg),before))}

def observe_point_append_hook(m,spec,method):
 v=m.VMobject();arg=arguments(spec);events=[];sentinel=object()
 def append(argument):
  a=np.asarray(argument);events.append({'isOriginal':argument is arg,'type':type(argument).__name__,'shape':list(a.shape),'dtype':a.dtype.name,'values':a.tolist()});return sentinel
 v.append_points=append
 try:
  result=getattr(v,method)(arg);out={'kind':'returned','returnsSelf':result is v,'returnsHookSentinel':result is sentinel}
 except Exception as e:out={'kind':'raised','type':type(e).__name__,'message':str(e)}
 return {'method':method,'spec':spec,'output':out,'appendHook':events}
