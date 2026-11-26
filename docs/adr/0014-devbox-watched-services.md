# Services the System monitor watches

## Context

The System monitor's "Background services" row shows two fixed things: a
`t3code.service` user unit and the voxtype dictation daemon's state file.
They were one developer's tools; on any other machine the row says "Off"
and "Offline" about programs that aren't there.

## Decision

- `devbox.watched` lists up to six things to watch, each one of:
  - `user_unit` / `system_unit`: a systemd service by its unit name,
    reported by `systemctl [--user] is-active` as running, starting,
    stopped or failed;
  - `container`: a Docker or Podman container by name, as `ps` reports it;
  - `port`: a local TCP port, open or closed by a connect to 127.0.0.1.
- The card shows each with its state. The full reading adds Start or
  Restart for user units and containers. System units need an
  administrator, so they are shown, not controlled.
- One status pass checks all of them with a bounded time each; nothing runs
  while the Services section is off.
- A saved System monitor with Services on and no list yet starts with the
  two it used to show, `t3code.service` and `voxtype.service`, so the row
  keeps its items. The voxtype state file is no longer read.

## Alternatives

- Keep the fixed pair and add a list beside it. Two hard-coded items stay
  wrong for everyone else.
- Arbitrary commands as checks. A shell command in the widget's settings is
  a way to run anything from a shared Look.

## Consequences

Watched items are personal: the list is a local field, never exported with
a Look. Names are checked (unit and container names, ports 1–65535) before
they reach a command, and are passed as arguments, never through a shell.
