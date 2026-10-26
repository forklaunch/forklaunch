# Unicode 16 default case mapping

The runtime tables target Node 24.2.0 / Unicode 16.0.0 `String.prototype.toLowerCase`
and `toUpperCase`, as used by Zod v3. They do not implement locale-specific casing,
Unicode normalization, or a representation for lone UTF-16 surrogates.

Source: https://www.unicode.org/Public/16.0.0/ucd/SpecialCasing.txt
SHA256: `8d5de354eef79f2395a54c9c7dcebbaf3d30fc962d0f85611ea97aa973a0c451`.
The accompanying Unicode License V3 applies to this data and generated tables.

`generate.py` verifies all source hashes before generating
`src/zod_string_case_tables.rs`. It reuses the repository's retained, unchanged
`vendor/regress/unicode-16.0.0/{UnicodeData,DerivedCoreProperties}.txt` by default.
For regeneration outside the repository, pass a directory containing those two
Unicode 16.0.0 files as its first argument. Their official source URLs are under
https://www.unicode.org/Public/16.0.0/ucd/; exact hashes are pinned in the generator.

The mapping uses simple UnicodeData mappings plus unconditional SpecialCasing
full mappings. The only language-independent conditional mapping is Final_Sigma,
using the pinned Cased and Case_Ignorable properties. Locale conditions are
excluded, as required for ECMAScript default casing.

The evaluator counts requested output bytes before allocation, debits cumulative
scan/output work, and uses fallible reservation. Sigma lookahead visits each
ignorable run at most once per mapping pass; repeated sigma input cannot cause
quadratic suffix traversal. This is a requested-work/allocation bound, not an
allocator-overhead or operating-system RSS guarantee. Invalid results retain no
public partial output; private dirty values remain available to intersections.
