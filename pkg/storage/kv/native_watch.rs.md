# Native watch

Each pull checks the leader read barrier, then reads the earliest matching
revision. A batch keeps the transaction's events together and carries its
response header separately from its cursor. Cancellation is terminal. A
watch holds a weak reference to the node and owns no background worker.
