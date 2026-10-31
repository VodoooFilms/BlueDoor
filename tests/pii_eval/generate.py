"""Authored fictional fixtures, CC0-1.0. No personal chats were collected.
Deterministic construction; no random source, network or model output.
"""
import json
from pathlib import Path
rows = []
values = {
 'rut': ['12.345.678-5', '9.876.543-3', '11.111.111-1', '12345678-5'],
 'cedula': ['1710034065', '0926687856', '0102030405', '1234567890'],
 'phone': ['+56 9 6123 4567', '09 8765 4321', '+593 99 123 4567', '(02) 2345-6789'],
 'iban': ['ES91 2100 0418 4502 0005 1332', 'GB82 WEST 1234 5698 7654 32', 'DE89 3704 0044 0532 0130 00', 'FR14 2004 1010 0505 0001 3M02 606'],
 'address': ['Av. Los Aromos 123, Santiago', 'Calle Ficticia 45, Quito', '123 Example Street, London', 'Pasaje Inventado 88, Lima'],
 'person': ['María José Pérez Soto', 'José Luis Muñoz Díaz', 'Ana Sofía Vera Ríos', 'James Alex Smith'],
 'email': ['ana@example.org', 'jose@gmial.com', 'maria@hotmial.con', 'alex+test@example.net'],
 'card': ['4111 1111 1111 1111', '5555-5555-5555-4444', '3782 822463 10005', '6011 1111 1111 1117'],
 'dob': ['03/04/90', '04-03-1990', '1990-04-03', '3.4.1990'],
}
labels = {'rut':'RUT', 'cedula':'cédula', 'phone':'teléfono', 'iban':'IBAN', 'address':'dirección', 'person':'nombre', 'email':'correo', 'card':'tarjeta', 'dob':'fecha de nacimiento'}
wrappers = ['{label}: {value}', 'Hola, mi {label}: {value}; gracias', '🔐**{label}: {value}**🙂', '> {label}: `{value}`', 'Please keep my {label}: {value}; private', 'Oye mi {label}: {value}\n¿lo recibiste?']
for category, choices in values.items():
 for value in choices:
  for template in wrappers:
   text = template.format(label=labels[category], value=value)
   rows.append({'id':f'{category}-{len(rows):03}', 'text':text, 'gold':[{'category':category, 'value':value}], 'negative':[]})
# Unlabeled conversational names: deliberately retain hard misses.
for name in values['person']:
 for template in ['Hablé con {} ayer.', '{} me pidió ayuda.', 'Can you call {} tomorrow?']:
  rows.append({'id':f'context-{len(rows)}', 'text':template.format(name), 'gold':[{'category':'person','value':name}], 'negative':[]})
for value in ['12345678', '987654321', '2026-10-31 14:35:22', '03/04/90', '4111 1111 1111 1112', 'ES90 2100 0418 4502 0005 1332']:
 for template in ['Pedido {} confirmado.', 'Order {} shipped.', 'Referencia técnica: {}.']:
  rows.append({'id':f'negative-{len(rows)}', 'text':template.format(value), 'gold':[], 'negative':[{'category':'non_pii','value':value}]})
Path(__file__).with_name('corpus.json').write_text(json.dumps(rows, ensure_ascii=False, indent=2)+'\n')
print(len(rows), 'fixtures')
