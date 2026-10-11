# yamlq

A `yq`-style YAML query/transform CLI — the `jq` idea, but for YAML
instead of JSON, with a plain dot/bracket path syntax instead of `jq`'s
own filter language.

## Installation

```sh
cargo install --path .
```

## Usage

```bash
yamlq get '.a.b[0].c' file.yaml
yamlq set '.port' 9090 file.yaml              # prints the updated doc to stdout
yamlq set '.port' 9090 file.yaml --in-place   # writes it back to file.yaml
yamlq set '.env' "8080" file.yaml --raw       # force a string instead of type-inferring a number
yamlq delete '.a.b' file.yaml --in-place
```

## Path syntax

Dot-separated map keys and bracketed array indices —
`.a.b[0].c` means "key `a`, then key `b`, then index `0` of that array,
then key `c`". A missing path is a clean error, not a panic or a silent
`null`.

## Type inference on `set`

`set` type-infers its value argument the same way a YAML scalar would:
`true`/`false` become booleans, `12` becomes a number, `null`/`~` becomes
null, and anything else stays a string — the same rule real YAML parsing
applies, so a piped-in value round-trips as the type you'd expect. Pass
`--raw`/`-r` to force it to stay a literal string regardless (so
`set .port "8080" -r` doesn't silently become the number `8080`).

## Status: built and verified against a realistic nested YAML file

- **28 unit tests** (`cargo test --lib`) across `path` (parsing dot/
  bracket syntax, including consecutive array indices and a malformed
  path producing a clean error) and `ops` (get/set/delete against nested
  maps, arrays, an array-index path, a path that doesn't exist producing
  a clean error rather than a panic, and setting a path that partially
  exists).
- **Live-verified against the actual compiled binary and a realistic
  multi-level YAML file** (nested maps, an array of objects, mixed
  scalar types): `get '.a.b[0].c'` returned the correct nested array
  element's field; `set '.port' 9090` correctly rewrote just that one
  scalar while leaving the rest of the document, including the nested
  array, untouched; `delete '.a.b'` correctly removed just that key,
  leaving the parent map present (as an empty map) rather than removing
  the whole `a:` section.

**Not done / deliberately deferred**: YAML anchors/aliases (`&foo`/`*foo`)
are read and preserved as `serde_yaml` resolves them, but there's no way
to *set* a path to reference an existing anchor; comments are not
preserved through a `set`/`delete` round trip (`serde_yaml` re-serializes
from the parsed value, which has no concept of comments — the same
limitation every `serde`-based YAML round-trip tool has); and there's no
`jq`-style filter/pipe language, only single-path get/set/delete.
