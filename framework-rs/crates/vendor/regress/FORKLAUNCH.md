# ForkLaunch bounded ECMAScript regex fork

This is the published `regress` 0.11.1 archive with reviewed Unicode16 semantic corrections and a bounded std+utf16 entry point. Its package is named `forklaunch-regress`; its library name remains `regress`. Original unlimited APIs remain upstream implementation details: ForkLaunch exposes only checked operations through `forklaunch-common::zod_regex`.

Upstream archive SHA256: `158a764437582235e3501f683b93a0a6f8d825d04a789dbe5ed30b8799b8908a`. The archive is retained at `provenance/regress-0.11.1.crate`. VCS metadata reports `7e64ad5e6807b5503e5cc97a79e0f129b23c556b`, dirty:true; the archive is authoritative. Original LICENSE-MIT and LICENSE-APACHE remain unchanged. Unicode input licensing is in `unicode-16.0.0/LICENSE.txt`.

`provenance/bounded-engine.patch` records the reviewed handwritten changes, and `unicode16.patch` records generated table/test changes. `SOURCE_SHA256SUMS` pins the reviewed source. Administrative package naming, portable generator paths, and this provenance documentation are subsequent packaging changes. The historical review manifest records original scratch evidence paths; permanent tests accompany the integration.

The bounded compiler checks requested collection storage and logical work before growth, with cumulative charges and typed resource failures. It does not bound allocator bookkeeping/over-allocation, process RSS, unrelated memory, wall-clock time or ordinary process OOM. The optimizer and start-predicate analysis are disabled in the limited path. Parser recursion is structurally limited and generated alternatives are balanced. Match budgets cover backtracking, candidate scans, capture state and nested lookaround. Resource exhaustion is never a normal false result.

Intended build: default-features=false, features=std,utf16. The published upstream no_std profile fails compilation; this fork does not claim to repair it. Existing unlimited APIs are not a framework capability.

Unicode16 inputs are retained for reproducibility. Their SHA256SUMS and the generator are adjacent. The target is Node24.2.0/Unicode16.0, not every future Node release. Pattern/input UTF16 units can represent lone surrogates; ordinary Rust String/JSON values have a narrower representation domain.
