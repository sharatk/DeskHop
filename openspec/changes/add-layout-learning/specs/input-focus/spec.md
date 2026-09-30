# Spec Delta

## MODIFIED Requirements

### Requirement: Edges that act as walls
An outer monitor side SHALL stop the cursor like a normal screen edge, with no crossing, while the layout maps it to no peer and layout learning does not place a peer there (see the `layout` capability), while the mapped peer is not connected, or while any mouse button is held.

#### Scenario: Dragging at the edge
- **WHEN** the user holds the left button to drag a window and pushes against a side mapped to connected peer B
- **THEN** focus stays local and the drag continues on this machine

#### Scenario: Peer not connected
- **WHEN** a side maps to peer B and B is not connected
- **THEN** pushing against that side does not change focus

#### Scenario: Unmapped side with nothing to learn
- **WHEN** a side maps to no peer and no connected peer is unplaced on this machine
- **THEN** pushing against that side does not change focus
