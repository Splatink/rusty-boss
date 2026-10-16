#!/usr/bin/env python3
"""Extract protocol facts as inert text; never eval or run vendor JavaScript.
Usage: python3 tools/recover_schema.py /path/to/extracted/html
"""
import ast
import hashlib
import json
from pathlib import Path
import re
import sys
from html.parser import HTMLParser

class Node:
    def __init__(self, attrs):
        self.attrs, self.children, self.text = dict(attrs), [], ''
    def all(self):
        yield self
        for c in self.children:
            yield from c.all()
    def content(self):
        return self.text + ''.join(c.content() for c in self.children)

class Layout(HTMLParser):
    def __init__(self):
        super().__init__(); self.root = Node([]); self.stack = [self.root]; self.ids = {}
    def handle_starttag(self, tag, attrs):
        n = Node(attrs); n.tag = tag; self.stack[-1].children.append(n)
        if 'id' in n.attrs: self.ids[n.attrs['id']] = n
        if tag not in ('input','img','br','hr','meta','link'): self.stack.append(n)
    def handle_endtag(self, tag):
        for i in range(len(self.stack)-1, 0, -1):
            if self.stack[i].tag == tag:
                del self.stack[i:]; break
    def handle_data(self, data): self.stack[-1].text += data

def linear(packed):
    b = packed.to_bytes(4, 'big')
    assert all(x < 128 for x in b), hex(packed)
    return sum(v << (7*(3-i)) for i,v in enumerate(b))

def recover(root):
    source = (root/'js/config/address_map.js').read_text(encoding='utf-8-sig')
    arrays = {}
    for match in re.finditer(r'(?:var\s+(\w+)|this\.(root))\s*=\s*\[(.*?)\];', source, re.S):
        rows = []
        for obj in re.findall(r'\{([^{}]+)\}', match[3]):
            row = {}
            for key,val in re.findall(r'(\w+)\s*:\s*(\'(?:[^\'\\]|\\.)*\'|[^,]+)', obj):
                val = val.strip()
                if val.startswith("'"): row[key] = ast.literal_eval(val)
                elif re.fullmatch(r'-?\d+|0x[\da-fA-F]+', val): row[key] = int(val,0) if val.startswith('0x') else int(val)
                elif re.fullmatch(r'\w+', val): row[key] = val
                else: raise ValueError(f'Unsupported literal: {val}')
            assert 'addr' in row and 'name' in row
            rows.append(row)
        arrays[match[1] or match[2]] = rows
    layout = Layout(); layout.feed((root/'export/layout.div').read_text())
    items = json.loads((root/'export/item.json').read_text())
    choices = {}
    for key,item in items.items():
        if 'pid' not in item: continue
        box = layout.ids.get(key+'-box')
        if not box: continue
        opts = [n.content().strip() for n in box.all() if n.tag == 'a']
        if not opts: continue
        order = item.get('list_order', list(range(len(opts))))
        assert len(opts) == len(order)
        choices[item['pid']] = [{'value':v,'label':label} for v,label in zip(order,opts)]
    blocks = []
    encodings = {'INTEGER1x7':('u7',1), 'INTEGER2x4':('nibbles',2), 'INTEGER2x7':('u14',2), 'INTEGER4x4':('nibbles',4)}
    for top in arrays['root']:
        if '(' in top['name']: continue # slot templates resolved in Rust
        for group in arrays[top['child']]:
            blockid = top['name']+'%'+group['name']; params=[]; size=0
            for p in arrays[group['child']]:
                encoding,width = encodings[p['size']] if isinstance(p['size'],str) else ('ascii',p['size'])
                offset=linear(p['addr']); size=max(size,offset+width)
                params.append(dict(name=p['name'],offset=offset,width=width,encoding=encoding,bias=p['ofs'],initial=p['init'],min=p['min'],max=p['max'],choices=choices.get(blockid+'%'+str(offset),[])))
            params.sort(key=lambda p:p['offset'])
            blocks.append(dict(id=blockid,address=linear(top['addr'])+linear(group['addr']),size=size,readonly=top['name'] in ('COMMAND','TEMP','SETUP') or 'READONLY' in group['name'],parameters=params))
    result = {'version':'1.1.0 (26)','model_id':[1,5,7],'blocks':blocks,
              'sources':{p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in ['js/config/address_map.js','export/item.json','export/layout.div']}}
    return result

if __name__ == '__main__':
    result = recover(Path(sys.argv[1]))
    out = Path(__file__).resolve().parents[1]/'data/katana-gen3.json'
    out.parent.mkdir(parents=True,exist_ok=True)
    out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(f'{len(result["blocks"])} blocks; {sum(len(b["parameters"]) for b in result["blocks"])} instantiated parameters -> {out}')
