# Native archive

The archive holds a checksummed view at one linearizable revision. Publishing a
file synchronizes its contents before publishing without overwriting a prior archive. The archive includes
KV metadata, the source consensus identity and the next lease ID.

Offline restore reserves a new directory and commits the archive to a fresh
one-voter consensus identity without opening sockets. It preserves durable KV
and the PD identity, allocator and TSO records. Lease-attached keys are removed,
lease IDs stay above the captured watermark and old watch cursors compact out.
