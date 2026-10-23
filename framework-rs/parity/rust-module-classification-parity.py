#!/usr/bin/env python3
"""Detect TS classification drift against canonical Rust module member mappings.
Only explicit TS scalar policies are compared. This does not infer relationships,
inherited SQL fields, provider-control tables or runtime encryption guarantees.
"""
from pathlib import Path
import re

repo=Path(__file__).resolve().parents[2]
modules={'iam/iam-base':'iam-base','billing/billing-base':'billing-base'}
count=0
for implementation,blueprint in modules.items():
    source=(repo/f'blueprint-rs/crates/implementations/{implementation}/src/storage_compliance.rs').read_text()
    mapped={}
    for block in re.finditer(r'impl ComplianceMetadata for \w+ \{(.*?)\n\}',source,re.S):
        entity=re.search(r'const ENTITY:.*?= "([^"]+)"',block[1]).group(1)
        fields={}
        for field in re.finditer(r'ComplianceField \{(.*?)\}',block[1],re.S):
            name=re.search(r'storage_name: "([^"]+)"',field[1]).group(1)
            level=re.search(r'level: ComplianceLevel::(\w+)',field[1]).group(1).lower()
            nullable=re.search(r'nullable: (true|false)',field[1]).group(1)=='true'
            fields[name]=(level,nullable)
        mapped[entity]=fields
    for path in (repo/f'blueprint/{blueprint}/persistence/entities').glob('*.entity.ts'):
        ts=path.read_text()
        match=re.search(r"name: '([^']+)'",ts)
        if not match or match[1] not in mapped:
            continue  # Other TS-only entities have no corresponding owned Rust model.
        expected={}
        for field in re.finditer(r'^    (\w+):([\s\S]*?)(?=^    \w+:|^  \})',ts,re.M):
            level=re.search(r"\.compliance\('([^']+)'\)",field[2])
            if level:
                expected[field[1]]=(level[1],'.nullable()' in field[2])
        assert mapped[match[1]]==expected,(path,mapped[match[1]],expected)
        count+=len(expected)
print(f'PASS: {count} explicit TS scalar classifications/nullability match canonical Rust module storage members')
