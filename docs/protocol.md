# DeskHop protocols

Two versioned protocols. A change to either bumps its version constant and updates this file in the same PR.

| Protocol | Between | Version constant | Current |
|---|---|---|---|
| Peer wire protocol | DeskHop peers on the LAN, over QUIC | `proto::PROTOCOL_VERSION` | 1 |
| IPC protocol | Service, agents, and UI, over named pipes | `ipc::IPC_VERSION` | 1 |

This document is written so that a third party could implement a compatible peer (ADR 0003, ADR 0004).

## Peer wire protocol

Shape: ADR 0004. Symmetric peers, mDNS/DNS-SD discovery with pair-by-code fallback, Noise pairing, QUIC with datagrams for motion and streams for everything else.

No messages are defined yet. Handshake pattern, framing, and datagram layout are settled in `openspec/` changes.

## IPC protocol

Shape: ADR 0002. Service, agents, and UI may run different versions during an upgrade.

No messages are defined yet.

## Version history

| Protocol | Version | Change |
|---|---|---|
| Peer | 1 | Initial version; no messages. |
| IPC | 1 | Initial version; no messages. |
