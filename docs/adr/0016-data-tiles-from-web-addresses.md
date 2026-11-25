# Data tiles from web addresses

## Context

ADR 0015 added Data tiles that read local JSON files, and left HTTP(S) for a
separate step. Many values people want on a tile already come from a JSON
address: a home server's status page, a CI result, a public counter. The
FB35 plan asks for this step to settle redirects, authentication and
response limits.

Halcyon already fetches calendar links with `calendar::http`. That client
allows HTTPS anywhere and plain HTTP only to this computer or the local
network. It checks every address a name resolves to and pins the
connection to them. It follows redirects by hand, checking each hop and
never moving from HTTPS to HTTP. It keeps a login only on its own host.

## Decision

- A tile's `source` may also be `{"kind": "http", "url": <address>}`. The
  address must be HTTPS, or plain HTTP to a local address or name. It must
  have no user name, password or fragment, and be at most 2048 characters.
- Web tiles read every 30 s at the most often; 10 s is refused for them.
- Fetching:
  - Requests go through `calendar::http::send`, so the rules above hold:
    pinned local HTTP, checked redirects, referrer off, a 20 s timeout.
  - `Request` gains a byte limit, so tiles keep the 256 KB limit while
    calendars keep theirs.
  - Each request is a `GET` with `Accept: application/json`.
  - Tiles sharing an address share one request per round.
- Answers:
  - A success is parsed as JSON like a file.
  - Any other status shows "The server answered N".
  - A network failure shows "Can’t reach the server".
  - Other refusals get their own short error: an address that isn't
    allowed, an HTTPS to HTTP redirect, an answer over 256 KB.
  - A web value has no file time, so it shows as stale only after a failed
    request, with the last value and its age.
- No sign-in in this step. Settings says not to use an address that carries
  a key. The address stays in local preferences, never in a Look or a usage
  report, as file paths do.
- Settings takes a file path or a web address in the same field. It reads
  either once to list its fields.

## Alternatives

- A second HTTP client for tiles. It would repeat the local-network and
  redirect checks that calendar links already rely on.
- Bearer tokens kept in the keyring now. Useful for Home Assistant and
  similar APIs, but it brings unlock prompts and per-address secrets into
  background polling; it deserves its own record once the unauthenticated
  path is in use.
- Keeping addresses in the keyring like private calendar links. Without
  sign-in, a tile address is not a credential; keeping it local and out of
  exports matches file paths.

## Consequences

- A web tile on a laptop away from home shows "Can’t reach the server" with
  its last value and age, rather than a blank.
- An API that needs a key in its address or a header can't be used yet.
- `calendar::http` is now shared and named for calendars only by history.
