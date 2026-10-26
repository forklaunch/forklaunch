# Unicode 16 emoji data

`emoji-data.txt` is unmodified Unicode 16.0.0 data from
https://www.unicode.org/Public/zipped/16.0.0/UCD.zip, member `emoji/emoji-data.txt`.
Archive SHA256: `c86dd81f2b14a43b0cc064aa5f89aa7241386801e35c59c7984e579832634eb2`.
Data SHA256: `f1365a5173eee18e1f98b240cdc492e84a25f1ce7e0c9d1094eb29c41a22696a`.

The accompanying Unicode License V3 was fetched from
https://www.unicode.org/license.txt on 2026-10-15. License SHA256: `e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96`.

`scripts/rust_zod_emoji_table.py` derives the union of Extended_Pictographic and
Emoji_Component for raw Zod emoji acceptance against Node 24.2.0 / Unicode 16.0.
The generated table is separately checked; it does not assert grapheme validity.
