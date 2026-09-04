---
tags:
  - multiplexing
  - ssh
---
# `mux_enable_ssh_agent = false`

{{since('nightly')}}

*The default for this option is `false` in sideterm; upstream wezterm
defaults it to `true`.*

When set to `true`, wezterm will configure the `SSH_AUTH_SOCK`
environment variable for panes spawned in the `local` domain.

The auth sock will point to a symbolic link that will in turn be pointed to the
authentication socket associated with the most recently active multiplexer
client.

You can review the authentication socket that will be used for various clients
by running `wezterm cli list-clients` and inspecting the `SSH_AUTH_SOCK`
column.

The symlink is updated within (at the time of writing this documentation) 100ms
of the active Mux client changing.

On Windows, Win32-OpenSSH cannot use this AF_UNIX socket, and overriding
`SSH_AUTH_SOCK` breaks the connection to the real system agent (the
`\\.\pipe\openssh-ssh-agent` named pipe), which is why sideterm defaults
this option to `false`.

Set `mux_enable_ssh_agent = true` only if you rely on agent forwarding
through `wezterm ssh` sessions.
