# srvcs-root

Arithmetic microservice for srvcs.cloud: the **nth root of a value**.

`srvcs-root` is an *orchestrator*. It owns the control flow but delegates every
arithmetic step to other srvcs services, computing

```
root(value, n) = value ^ (1 / n)
```

as a `f64` (a JSON number that may be fractional).

## Concern

- **service**: `srvcs-root`
- **concern**: `arithmetic: nth root of value`
- **depends_on**: `srvcs-floatpower`, `srvcs-floatdivide`

## API

### `GET /`

Service identity.

```json
{
  "service": "srvcs-root",
  "concern": "arithmetic: nth root of value",
  "depends_on": ["srvcs-floatpower", "srvcs-floatdivide"]
}
```

### `POST /`

Request:

```json
{ "value": 27, "n": 3 }
```

Response `200`:

```json
{ "value": 27.0, "n": 3.0, "result": 3.0 }
```

The `result` is an `f64` and may be fractional; e.g. `root(16, 2) == 4.0`.

## Algorithm

1. `exp = (call srvcs-floatdivide { "a": 1, "b": n }).result`
2. `result = (call srvcs-floatpower { "base": value, "exp": exp }).result`

`srvcs-root` does **not** call `srvcs-isnumber` directly — input validation
propagates from its dependencies. `srvcs-floatpower` returns `422` if the
result is not real (e.g. an even root of a negative value); `srvcs-root`
forwards that `422`.

### Status codes

- `200` — computed result.
- `422` — a dependency rejected the input (forwarded verbatim).
- `500` — a dependency returned a malformed result (contract violation).
- `503` — a dependency is unavailable (degraded).

## Configuration

| Variable                | Default                 | Purpose                       |
| ----------------------- | ----------------------- | ----------------------------- |
| `SRVCS_BIND_ADDR`       | `0.0.0.0:8080`          | Listen address.               |
| `SRVCS_FLOATPOWER_URL`  | `http://127.0.0.1:8090` | Base URL of `srvcs-floatpower`. |
| `SRVCS_FLOATDIVIDE_URL` | `http://127.0.0.1:8091` | Base URL of `srvcs-floatdivide`. |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

See [`srvcs/platform`](https://github.com/srvcs/platform) for the shared service
standard and CI workflow.
