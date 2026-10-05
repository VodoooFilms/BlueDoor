#!/usr/bin/env python3
"""Independently verify output bytes, visible redaction and unchanged pixels."""
import base64
import io
import json
from pathlib import Path
import subprocess
from PIL import Image, ImageChops, ImageDraw
from pypdf import PdfReader

root = Path('artifacts/layout-workflow')
for name in ['layout-document.pdf', 'private-scan.pdf']:
    plan = json.loads((root / f'{name}.plan.json').read_text())
    preview = json.loads((root / f'{name}.preview.json').read_text())
    output = root / f'{name}.protected.pdf'
    pdf = PdfReader(output)
    assert len(pdf.pages) == len(plan['pages'])
    assert all(not (page.extract_text() or '').strip() for page in pdf.pages), 'Original selectable text layer survived'
    assert not pdf.trailer['/Root'].get('/Metadata'), 'Source metadata survived'
    for number, (source, protected) in enumerate(zip(plan['pages'], preview)):
        page = pdf.pages[number]
        assert abs(float(page.mediabox.width) - source['width']) < .01
        assert abs(float(page.mediabox.height) - source['height']) < .01
        assert not page.get('/Annots'), 'Source links/widgets survived'
        before = Image.open(io.BytesIO(base64.b64decode(source['imageBase64']))).convert('RGB')
        after = Image.open(io.BytesIO(base64.b64decode(protected['imageBase64']))).convert('RGB')
        assert before.size == after.size
        mask = Image.new('L', before.size, 0); painter = ImageDraw.Draw(mask)
        for area in plan['redactions']:
            if area['page'] != number: continue
            x,y,w,h = area['rect']; sx,sy = before.width/source['width'],before.height/source['height']
            painter.rectangle((int(x*sx)-2,int(y*sy)-2,int((x+w)*sx)+2,int((y+h)*sy)+2),fill=255)
        difference = ImageChops.difference(before, after)
        difference.paste((0,0,0), mask=mask)
        assert difference.getbbox() is None, 'Graphics or layout changed outside private regions'
        after.save(root/f'{name}.page-{number+1}.png')
        embedded = list(page.images)
        assert len(embedded) == 1, 'Unexpected source content survived'
        exported = embedded[0].image.convert('RGB')
        assert exported.size == after.size
        assert ImageChops.difference(exported,after).getbbox() is None, 'Export differs from preview'
    helper = Path('src-tauri/resources/runtime/belegal-ocr')
    extracted = subprocess.check_output([str(helper), str(output)],stderr=subprocess.DEVNULL).decode()
    for value in ['María Pérez','Maria Perez','maria@example.com','0912345678']:
        assert value.casefold() not in extracted.casefold(), f'Redacted value visible to independent OCR: {value}'
    assert 'PERSON' in extracted and 'ID' in extracted
    assert '1500' in extracted, 'Unrelated monetary value was lost'
    print(f'PASS {name}: page geometry, unchanged graphics, preview=export, no original text/links/metadata, independent OCR')

# Rich correction: independently compare against the already protected pages,
# not the source, so every unchanged alias and graphic must remain identical.
plan = json.loads((root / 'rich-edited.plan.json').read_text())
preview = json.loads((root / 'rich-edited.preview.json').read_text())
baseline = json.loads((root / 'layout-document.pdf.preview.json').read_text())
pdf = PdfReader(root / 'rich-edited.pdf')
assert len(pdf.pages) == 2
for number, (before_page, after_page) in enumerate(zip(baseline, preview)):
    before = Image.open(io.BytesIO(base64.b64decode(before_page['imageBase64']))).convert('RGB')
    after = Image.open(io.BytesIO(base64.b64decode(after_page['imageBase64']))).convert('RGB')
    mask = Image.new('L', before.size, 0)
    painter = ImageDraw.Draw(mask)
    for overlay in plan['overlays']:
        if overlay['page'] != number: continue
        for rect in [overlay['rect'], overlay['eraseRect']]:
            x, y, w, h = rect
            sx, sy = before.width / before_page['width'], before.height / before_page['height']
            painter.rectangle((int(x*sx)-2, int(y*sy)-2, int((x+w)*sx)+2, int((y+h)*sy)+2), fill=255)
    difference = ImageChops.difference(before, after)
    difference.paste((0,0,0), mask=mask)
    assert difference.getbbox() is None, 'Rich correction moved an unchanged graphic, text or alias'
    assert abs(float(pdf.pages[number].mediabox.width)-before_page['width']) < .01
    assert abs(float(pdf.pages[number].mediabox.height)-before_page['height']) < .01
    assert not pdf.pages[number].extract_text().strip(), 'Source text layer survived rich export'
    assert ImageChops.difference(list(pdf.pages[number].images)[0].image.convert('RGB'),after).getbbox() is None, 'Rich preview differs from exported PDF'
    after.save(root / f'rich-edited.page-{number+1}.png')
extracted = subprocess.check_output([str(helper), str(root/'rich-edited.pdf')], stderr=subprocess.DEVNULL).decode()
assert 'Informe corregido' in extracted
assert 'Informe de prueba' not in extracted
assert 'María Pérez' not in extracted and 'maria@example.com' not in extracted
assert '1500' in extracted
print('PASS rich correction: visible corrected text, unchanged graphics/aliases/margins, preview=export, original text absent')

# Complete paragraphs may reflow inside their available space. Table borders,
# illustrations and all pixels outside the edited region must remain unchanged.
plan = json.loads((root / 'paragraph-plan.json').read_text())
before = json.loads((root / 'paragraph-before.preview.json').read_text())
after = json.loads((root / 'paragraph-edited.preview.json').read_text())
pdf = PdfReader(root / 'paragraph-edited.pdf')
assert len(pdf.pages) == len(before) == len(after) == 2
for number, (original, edited) in enumerate(zip(before, after)):
    baseline = Image.open(io.BytesIO(base64.b64decode(original['imageBase64']))).convert('RGB')
    applied = Image.open(io.BytesIO(base64.b64decode(edited['imageBase64']))).convert('RGB')
    mask = Image.new('L', baseline.size, 0)
    painter = ImageDraw.Draw(mask)
    sx, sy = baseline.width / original['width'], baseline.height / original['height']
    for overlay in plan['overlays']:
        if overlay['page'] != number:
            continue
        for key in ['eraseRect', 'rect']:
            x, y, w, h = overlay[key]
            painter.rectangle((int(x*sx)-2, int(y*sy)-2, int((x+w)*sx)+2, int((y+h)*sy)+2), fill=255)
    difference = ImageChops.difference(baseline, applied)
    difference.paste((0, 0, 0), mask=mask)
    assert difference.getbbox() is None, 'Paragraph edit changed unrelated content'
    page = pdf.pages[number]
    assert not (page.extract_text() or '').strip(), 'Original text layer survived paragraph export'
    assert not page.get('/Annots')
    assert abs(float(page.mediabox.width) - original['width']) < .01
    assert abs(float(page.mediabox.height) - original['height']) < .01
    exported = list(page.images)[0].image.convert('RGB')
    assert ImageChops.difference(exported, applied).getbbox() is None, 'Paragraph preview differs from export'
print('PASS paragraphs: unchanged table/illustration/other paragraphs/pages, geometry, preview=export, no source text layer')
