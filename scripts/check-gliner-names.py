"""Create synthetic ONNX regression inputs for gliner::tests::real_name_coverage.
Run with the local gliner QA venv; no document or network access is used.
"""
import argparse
import json
from pathlib import Path
import re
import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

parser = argparse.ArgumentParser()
parser.add_argument('--model-dir', type=Path, default=Path('.build-cache/gliner'))
parser.add_argument('--multi', action='store_true')
parser.add_argument('--multi-file', default='model_fp16.onnx')
parser.add_argument('--person-label', default='person name')
parser.add_argument('--output', type=Path, default=Path('artifacts/gliner-name-regression.json'))
args = parser.parse_args()
tokenizer = Tokenizer.from_file(str(args.model_dir / 'tokenizer.json'))
options = ort.SessionOptions()
options.intra_op_num_threads = 4
session = ort.InferenceSession(str(args.model_dir / (args.multi_file if args.multi else 'onnx/model_quint8.onnx')), options, providers=['CPUExecutionProvider'])
cases = [
    ('Carlos Andrés Mendoza\nVALDEZ\nTotal: USD 20', ['Carlos Andrés Mendoza\nVALDEZ'], ['Total: USD 20']),
    ('Nombre: Elena Sofía Vargas\nRÍOS', ['Elena Sofía Vargas\nRÍOS'], ['Nombre:']),

    ('Nombre: Carlos Andrés Mendoza VALDEZ', ['Carlos Andrés Mendoza VALDEZ'], ['Nombre:']),
    ('CARLOS ANDRÉS MENDOZA VALDEZ comparece ante la autoridad.', ['CARLOS ANDRÉS MENDOZA VALDEZ'], [' comparece ante la autoridad.']),
    ('Ana Sofía Ramírez Cedeño presenta la solicitud.', ['Ana Sofía Ramírez Cedeño'], [' presenta la solicitud.']),
    ('María del Carmen de la Cruz Paredes comparece.', ['María del Carmen de la Cruz Paredes'], [' comparece.']),
    ('Nombre: josé luis pérez quiñónez', ['josé luis pérez quiñónez'], ['Nombre:']),
    ('Elena Vargas-Morales Ríos y Pedro Núñez Salazar comparecen.', ['Elena Vargas-Morales Ríos', 'Pedro Núñez Salazar'], [' y ', ' comparecen.']),

    ('Nombre: Diego Alfonso Herreera Lozano\nTotal USD 1500', ['Diego Alfonso Herreera Lozano'], ['Total USD 1500']),
    ('DIEGO ALFONSO HERREERA LOZANO', ['DIEGO ALFONSO HERREERA LOZANO'], []),
    ('Diego Alfonso Herrera Lozano comparece ante la autoridad.', ['Diego Alfonso Herrera Lozano'], ['comparece ante la autoridad.']),
    ('Nombre: diego alfonso herreera lozano', ['diego alfonso herreera lozano'], ['Nombre:']),
    ('Nombre: María José de la Torre Vera', ['María José de la Torre Vera'], ['Nombre:']),
    ('Diego Alfonso Herrera Lozano y Ana Lucia Vera comparecen.', ['Diego Alfonso Herrera Lozano', 'Ana Lucia Vera'], [' y ', ' comparecen.']),
]
result = []
for text, names, preserved in cases:
    words = list(re.finditer(r'\w+(?:[-_]\w+)*|\S', text))
    # Rust offsets are UTF-8 bytes rather than Python character indices.
    offsets = [[len(text[:word.start()].encode()), len(text[:word.end()].encode())] for word in words]
    outputs = []
    for person in ['person', args.person_label]:
        labels = [person, 'organization', 'national identification number', 'email address', 'phone number', 'address', 'case number', 'bank account number']
        prefix = [value for label in labels for value in ['<<ENT>>', label]] + ['<<SEP>>']
        surface = [word.group() for word in words]
        if person != 'person':
            surface = [word[:1] + word[1:].lower() if word.isupper() and sum(c.isalpha() for c in word) >= 2 else word for word in surface]
        encoded = tokenizer.encode(prefix + surface, is_pretokenized=True)
        previous = None
        mask = []
        for word in encoded.word_ids:
            mask.append(word - len(prefix) + 1 if word is not None and word >= len(prefix) and word != previous else 0)
            previous = word
        ids = np.array([encoded.ids], dtype=np.int64)
        inputs = {'input_ids': ids, 'attention_mask': np.ones_like(ids), 'words_mask': np.array([mask], dtype=np.int64), 'text_lengths': np.array([[len(words)]], dtype=np.int64)}
        if args.multi:
            assert len(encoded.ids) <= 384, 'regression fixture exceeds model context'
            spans = [(start, start + width) for start in range(len(words)) for width in range(12)]
            inputs['span_idx'] = np.array([spans], dtype=np.int64)
            inputs['span_mask'] = np.array([[end < len(words) for start, end in spans]], dtype=bool)
        logits = session.run(['logits'], inputs)[0]
        outputs.append({'shape': list(logits.shape), 'logits': logits.flatten().tolist()})
    result.append({'text': text, 'names': names, 'preserved': preserved, 'words': offsets, 'outputs': outputs})
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(result, ensure_ascii=False))
print(f'Generated {len(result)} synthetic real-model regression cases: {args.output}')
