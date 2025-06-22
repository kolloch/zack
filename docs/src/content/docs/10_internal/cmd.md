---
title: cmd in Starlark
description: How to express running commands in Starlark
---

For running a command, we need

- the command (input file but exec config),
- the args (strings),
- the input files and their path

# Repeated strings

```starlark
cmd("gcc", "-o", out("main.o"), in("main.cc"))

cmd("gcc", "-o", out("main"), in("main.o"))
```

# Strings as variables

```starlark
main_o = "main.o"

cmd("gcc", "-o", out(main_o), in("main.cc"))

cmd("gcc", "-o", out("main"), in(main_o))
```