# DeskHop threat model

*Status: skeleton. Filled in alongside the `pairing`, `transport`, and `ipc` specs.*

DeskHop injects keystrokes received over the network, and its service runs as SYSTEM. This document records what it defends against and where.

## Assets

- Identity keys and the trust store (service).
- Keystrokes and clipboard contents in transit.
- The ability to inject input into a machine, including the secure desktop.

## Trust boundaries

| Boundary | Less trusted side | Notes |
|---|---|---|
| LAN → service | Any host on the network | Unauthenticated packets reach `proto` decoding as SYSTEM. |
| Named pipe → service | Agents, UI | The service validates everything it receives (ADR 0002). |
| Service → winlogon agent | — | Agent runs as SYSTEM on the secure desktop. |

## Threats

To be written.
