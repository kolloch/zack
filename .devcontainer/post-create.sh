#/usr/bin/env bash

. "$HOME/.cargo/env"

jj config set --user user.name "$(git config user.name)"
jj config set --user user.email "$(git config user.email)"

jj config set --user ui.editor vim
