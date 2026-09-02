# Postmortems

One file per failure, injected or organic. Week 3 requires two: Kafka killed
mid-write, and ClickHouse throttled or stalled.

Each entry: what broke, how it was detected (which metric moved), the blast
radius, whether data was lost or double-counted on recovery, what the fix was,
and what would have caught it earlier.

Write these even when the failure was deliberate and the outcome expected. The
value is in the recovery path being proven rather than assumed.

---

_No entries yet._
