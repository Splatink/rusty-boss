#!/usr/bin/env python3
"""Recover UI bindings, labels, finite option tables and chain facts WITHOUT JS eval.
Usage: python3 tools/recover_editor.py /path/to/extracted/html
"""
import ast
import hashlib
import json
from pathlib import Path
import re
import sys
from recover_schema import Layout


def recover(root):
    layout = Layout()
    layout.feed((root/'export/layout.div').read_text())
    items = json.loads((root/'export/item.json').read_text())
    controls = {}
    for key, item in items.items():
        pid = item.get('pid', '')
        n = layout.ids.get(key)
        if not n or not pid.startswith(('PATCH%', 'SYSTEM%')):
            continue
        classes = n.attrs.get('class', '')
        if 'item-hidden' in classes or not any('elf-'+s+'-control' in classes for s in ['dial','knob','slider','select-box','check-box','radio-button','toggle-button']):
            continue
        block, offset = pid.rsplit('%', 1)
        if '=' in offset or not offset.isdigit():
            continue
        label_node = layout.ids.get(n.attrs.get('description', ''))
        if not label_node:
            label_node = layout.ids.get(re.sub(r'-(?:dial|knob|select-box|slider|btn)$', '-label', key))
        label = label_node.content().strip() if label_node else key.replace('-', ' ').upper()
        label = {"panel-cab-resonance-select-box":"CABINET RESONANCE", "panel-amp-type-vari-btn":"VARIATION", "asgn-knobs-booster-select-box":"BOOSTER KNOB", "asgn-knobs-mod-select-box":"MOD KNOB", "asgn-knobs-fx-select-box":"FX KNOB", "asgn-knobs-delay-select-box":"DELAY KNOB", "asgn-knobs-reverb-select-box":"REVERB KNOB", "asgn-pedal-function-select-box":"EXPRESSION PEDAL FUNCTION", "asgn-tip-function-select-box":"FOOTSWITCH TIP", "asgn-ring-function-select-box":"FOOTSWITCH RING"}.get(key,label)
        choices = []
        box = layout.ids.get(key+'-box')
        if box:
            labels = [c.content().strip() for c in box.all() if c.tag == 'a']
            order = item.get('list_order', list(range(len(labels))))
            assert len(labels) == len(order), key
            choices = [dict(value=v,label=t) for v,t in zip(order, labels)]
        stringer = layout.ids.get(item.get('stringer', ''))
        fmt = stringer.attrs.get('format', '') if stringer else ''
        if stringer and not fmt:
            labels = [c.content().strip() for div in stringer.children if div.tag == 'div' for c in div.children if c.tag == 'p']
            if labels:
                choices = [dict(value=i,label=t) for i,t in enumerate(labels)]
        # Finite known display rules only. Unknown expressions stay raw; NEVER eval.
        display = 'raw'
        if 'dB' in fmt: display = 'half_db' if 'value/2' in fmt else 'db'
        elif 'ms' in fmt: display = 'ms'
        elif "'s'" in fmt and 'value/10' in fmt: display = 'tenths_s'
        elif "'Hz'" in fmt: display = 'hz'
        elif "'cm'" in fmt: display = 'cm'
        elif "'%'" in fmt: display = 'double_percent' if 'value * 2' in fmt else 'percent'
        elif 'OFF' in fmt: display = 'off_negative'
        elif fmt == 'value + 1': display = 'plus_one'
        elif "'+'" in fmt: display = 'signed'
        controls[key] = dict(id=key,block=block,offset=int(offset),label=label,choices=choices,display=display,
                             min=int(n.attrs['min']) if n.attrs.get('min','').lstrip('-').isdigit() else None,
                             max=int(n.attrs['max']) if n.attrs.get('max','').lstrip('-').isdigit() else None,
                             bias=item.get('vofs',0))
        c = controls[key]
        # A shared stringer may contain more entries than this widget permits
        # (notably solo-delay-high-cut). Respect the shipped widget bounds.
        c['choices'] = [o for o in choices if (c['min'] is None or o['value'] >= c['min']) and (c['max'] is None or o['value'] <= c['max'])]
    groups = {}
    for key,n in layout.ids.items():
        ids = [c.attrs.get('id') for c in n.all() if c.attrs.get('id') in controls]
        if ids: groups[key] = ids
    selectors = {}
    for key,item in items.items():
        if key not in controls or not item.get('frame'): continue
        frame = layout.ids[item['frame']]
        pages = [c.attrs.get('id','') for c in frame.children]
        selectors[key] = dict(pages=pages,order=item.get('order',list(range(len(pages)))))
    chain_text = (root/'js/businesslogic/chain/chain_config.js').read_text()
    chains = []
    for m in re.finditer(r'\{ pdlVal: (\d), srVal: (\d), list:\s*(\[.*?\])\s*\}',chain_text,re.S):
        flows = ast.literal_eval(m[3])
        assert len(flows) == 9 and all(len(f)==10 for f in flows)
        chains.append(dict(pedal=int(m[1]),send_return=int(m[2]),flows=flows))
    assert len(chains)==6
    return dict(controls=controls,groups=groups,selectors=selectors,chains=chains,
                sources={p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in ['export/item.json','export/layout.div','js/item/item_logic.js','js/businesslogic/chain/chain_config.js','js/businesslogic/bts/effect_controller.js','js/businesslogic/ktn/model_info.js']})

if __name__ == '__main__':
    result = recover(Path(sys.argv[1]))
    out = Path(__file__).resolve().parents[1]/'data/editor-ui.json'
    # Only retain groups actually used by the Rust presentation, plus selector pages.
    wanted = {'booster-content-page','editor-ns-page','editor-sr-page','editor-solo-page','editor-contour-page',
              'asgn-knobs-pedal-page','asgn-gafc-page','editor-effects-page'}
    wanted.update(page for s in result['selectors'].values() for page in s['pages'])
    wanted.update(['pedalfx-position-select-box','eq-position-select-box','eq2-position-select-box'])
    result['groups'] = {k:v for k,v in result['groups'].items() if k in wanted}
    out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(f'{len(result["controls"])} controls, {len(result["groups"])} groups, 54 chain arrangements -> {out}')
