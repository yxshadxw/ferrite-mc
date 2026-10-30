# Fluid region ownership

The regionizer groups held chunk sections within its merge radius into one region. Fluid slope searches inspect at most four horizontal blocks from the ticking position (plus a vertical neighbor), so any held chunk reached by a tick is either in that region or absent; additions touching a ticking region wait for its safe point.

A region must only read or mutate chunks it currently holds. Cached columns are not proof of ownership: a released column can remain in the shared Anvil cache. Region membership follows holder additions and removals, and its pending fluid ticks move with their positions when regions merge or split. An absent chunk is an impassable boundary and is never loaded by a fluid tick; changes and notifications for another region must be posted to that chunk's region queue.
