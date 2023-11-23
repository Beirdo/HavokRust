#! /usr/bin/env python3

import json
import sys

if len(sys.argv) < 3:
    print("Usage: %s json-key-file privateKey" % sys.argv[0])
    sys.exit(1)

with open(sys.argv[1]) as f:
    data = json.load(f)

data = {row[0]: row[1] for row in data}

privkey = data.get(sys.argv[2], "")
if not privkey:
    sys.exit(2)

print(privkey)
sys.exit(0)
