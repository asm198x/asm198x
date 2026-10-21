# #919 qualification checkpoint

Source 9e448f2c496c5c6641164ddb8097f34f28cf4bf9 is pushed and has passed
1,642 workspace tests (38 ignored), Lua Clippy and 115 Node/setup tests. Its
clean-source release executable matches the frozen producer exactly. The
local archive passes all 103 cohorts, with wrong bytes, corrupt checksum and
#916 omission controls failing exactly as intended. Fresh 4510 replay passes
all 564 bytes and fourteen streams; its work directory was removed.

The actual x86-64 Linux, ARM Linux and Apple Silicon macOS reports pass all
103 cohorts, including 5,441 external-binding commands each. Intel macOS,
Windows and coverage are still pending in runs 39580444755 / 39580445974.
This is an incomplete qualification checkpoint, not permission to merge.

The new frozen producer inventories 26 matrices / 4,136 cases for the next
slice. Its measured next baselines have 186 differences in 515 data cases and
30 differences in 50 numeric-consumer cases. The next plan and their native
facts are retained here. The existing task worktree remains active.
