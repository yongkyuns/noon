"""Generate geometric Table query fixtures with actual pinned ManimCE 0.21.0."""
import argparse
import hashlib
import json
from pathlib import Path
import manim as m

assert m.__version__ == "0.21.0"

def observe(case):
    nrows,ncols=case.get('body_rows',2),case.get('body_cols',2)
    cells=[[m.Square(side_length=.4+.1*(ncols*r+c)) for c in range(ncols)] for r in range(nrows)]
    rows=[m.Circle(radius=.2+.1*i) for i in range(max(nrows,case.get('row_count',nrows)))]
    cols=[m.Triangle().scale(.7+.1*i) for i in range(max(ncols,case.get('col_count',ncols)))]
    corner=m.Dot()
    names={id(value):f'cell{r}{c}' for r,row in enumerate(cells) for c,value in enumerate(row)}
    names.update({id(value):f'row{i}' for i,value in enumerate(rows)})
    names.update({id(value):f'col{i}' for i,value in enumerate(cols)})
    names[id(corner)]='corner'
    kwargs={}
    if case.get('rows'):kwargs['row_labels']=rows[:case.get('row_count',nrows)]
    if case.get('cols'):kwargs['col_labels']=cols[:case.get('col_count',ncols)]
    if case.get('corner'):kwargs['top_left_entry']=corner
    if case.get('alias'):cells[1][1]=cells[0][0]
    if case.get('same_row_alias'):cells[0][1]=cells[0][0]
    if case.get('same_column_alias'):cells[1][0]=cells[0][0]
    if case.get('all_alias'):cells=[[cells[0][0]]*ncols for _ in range(nrows)]
    if case.get('family_overlap'):
        family=m.VGroup(cells[0][0],cells[0][1])
        names[id(family)]='family00'
        cells[0][0]=family
    if case.get('omit_inner'):kwargs['include_inner_lines']=False
    def name(value):
        if id(value) in names:return names[id(value)]
        return {'type':type(value).__name__,'points':len(value.get_all_points())}
    def read(call,convert=lambda value:value):
        try:return convert(call())
        except Exception as error:return {'exception':type(error).__name__,'message':str(error)}
    initial_columns=[{'center':[float(v) for v in value.get_center()[:2]],'width':float(value.width),'height':float(value.height)} for value in cols]
    try:table=m.MobjectTable(cells,**kwargs)
    except Exception as error:return {'initial_columns':initial_columns,'case':case,'constructor_exception':type(error).__name__,'message':str(error)}
    result={'initial_columns':initial_columns,'case':case,'rows':read(table.get_rows,lambda groups:[[name(value) for value in row] for row in groups]),'columns':read(table.get_columns,lambda groups:[[name(value) for value in column] for column in groups]),'entries':read(table.get_entries,lambda group:[name(value) for value in group]),'body_entries':read(table.get_entries_without_labels,lambda group:[name(value) for value in group]),'labels':read(table.get_labels,lambda group:[name(value) for value in group]),'line_counts':[len(table.get_horizontal_lines()),len(table.get_vertical_lines())]}
    result['entry_positions']={str(pos):read(lambda pos=pos:table.get_entries(pos),name) for pos in [(1,1),(1,2),(2,1),(2,2),(3,3),(0,0),(-1,-1),(99,99)]}
    result['body_positions']={str(pos):read(lambda pos=pos:table.get_entries_without_labels(pos),name) for pos in [(1,1),(2,2),(3,3),(0,0)]}
    result['cells']={str(pos):read(lambda pos=pos:table.get_cell(pos),lambda value:{'center':[float(x).hex() for x in value.get_center()[:2]],'width':float(value.width).hex(),'height':float(value.height).hex()}) for pos in [(1,1),(2,2),(3,3)]}
    first=table.get_rows()[0][0]
    result['first_grid_member_identity_stable']=first is table.get_rows()[0][0]
    return result


CASES = [{},
 {'rows': True},
 {'cols': True},
 {'rows': True, 'cols': True},
 {'rows': True, 'cols': True, 'corner': True},
 {'corner': True},
 {'cols': True, 'corner': True},
 {'rows': True, 'corner': True},
 {'rows': True, 'cols': True, 'omit_inner': True},
 {'alias': True},
 {'rows': True, 'row_count': 1},
 {'cols': True, 'col_count': 1},
 {'cols': True, 'body_cols': 3},
 {'rows': True, 'body_rows': 3},
 {'rows': True, 'row_count': 0},
 {'cols': True, 'col_count': 0},
 {'rows': True, 'cols': True, 'row_count': 0},
 {'rows': True, 'cols': True, 'col_count': 0},
 {'rows': True, 'cols': True, 'row_count': 0, 'col_count': 0},
 {'rows': True, 'row_count': 3},
 {'cols': True, 'col_count': 3},
 {'rows': True, 'cols': True, 'row_count': 3},
 {'rows': True, 'cols': True, 'col_count': 3},
 {'same_row_alias': True},
 {'same_column_alias': True},
 {'all_alias': True},
 {'family_overlap': True},
 {'same_row_alias': True, 'rows': True, 'cols': True},
 {'alias': True, 'rows': True, 'cols': True, 'corner': True}]

def numeric(value):
    if isinstance(value,dict):return {key:numeric(item) for key,item in value.items()}
    if isinstance(value,list):return [numeric(item) for item in value]
    if isinstance(value,str) and value.startswith(("0x","-0x")):return float.fromhex(value)
    return value

if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    args=parser.parse_args()
    with m.tempconfig({"progress_bar":"none","verbosity":"ERROR"}):
        cases=[numeric(observe(case)) for case in CASES]
    source=Path(m.__file__).parent/"mobject/table.py"
    report={"schemaVersion":1,"manimVersion":m.__version__,"manimTableSourceSha256":hashlib.sha256(source.read_bytes()).hexdigest(),"generatorSha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),"scope":"Geometric table constructor/query and alias observations; bounds use a declared 1e-6 comparison budget in native tests.","cases":cases}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+"\n")
    print(f"Observed {len(cases)} pinned geometric Table cases")
