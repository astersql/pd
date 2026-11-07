# Native MVCC state

A selected transaction branch validates its leases before changing keys.
A mutation that changes the store advances the KV revision once. Reads within a
transaction see its preceding operations. Byte-ordered histories are pinned by
revision and the compacted boundary retains the floor version of every live key.
Events and previous delete values derive from the same committed commands.

A lease ID is allocated by the state machine and never reused. Only the leader
proposes clock advances from elapsed monotonic time and the state machine
applies them in log order. Lease time may pause during loss of quorum or
leadership change. Each revoked or expired lease deletes its attached keys
atomically and produces their events at one revision.
