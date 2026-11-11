# Clippy fix task: `crates/openduck/src/config/proxy.rs` string_slice errors

Repo: **/mnt/e/openduck-harness**

`cargo clippy -p openduck --all-targets -- -D warnings` fails on SIX `clippy::string_slice`
errors, all in `crates/openduck/src/config/proxy.rs` at lines 136, 140, 158 (×2), 169, 185.

These are byte-index slices of `&str` taken from `find()` offsets. They are char-boundary safe
(`find` returns a valid boundary), but clippy's `string_slice` lint fires because indexing can
panic on multi-byte UTF-8. Fix WITHOUT changing behavior or public API, using boundary-safe
APIs instead of `a[..i]` / `a[i..]`:

- `split_scheme` — use `split_once("://")`:
  ```rust
  fn split_scheme(value: &str) -> Option<(&str, &str)> {
      let (scheme, rest) = value.split_once("://")?;
      if !is_scheme_token(scheme) {
          return None;
      }
      Some((scheme, rest))
  }
  ```
  (equivalent: `find("://")` then scheme = everything before, rest = everything after the separator)
- `split_authority` — replace `(&rest[..index], &rest[index..])` with `rest.split_at(index)`.
- `strip_userinfo` — replace `&authority[index + 1..]` with `authority.split_at(index + 1).1`.
- IPv6 bracket handling near line 185 — replace `&inside[end + 1..]` with
  `inside.split_at(end + 1).1`.

Rules:
- ONLY edit `crates/openduck/src/config/proxy.rs`. Do not touch any other file.
- Do NOT change validation semantics, error messages, redaction behaviour, or test expectations.
- Do NOT silence the lint with `#[allow(...)]`; fix the code.
- Run `rustfmt --edition 2021 crates/openduck/src/config/proxy.rs` afterwards.

Verification (report the exact output lines):
1. `cargo test -p openduck --lib config::proxy` — all 15 tests must still pass.
2. `cargo clippy -p openduck --all-targets -- -D warnings 2>&1 | grep -v jemalloc | grep -E '^error|--> '`
   — the proxy.rs errors must be gone. NOTE: one pre-existing unrelated error remains at
   `crates/openduck/src/providers/inventory/mod.rs:1417` (`needless_borrow`) — that file is
   unmodified by this work and is out of scope. Confirm it is the ONLY remaining error and
   report that. It is a known pre-existing repo/toolchain issue, not something you may fix.
