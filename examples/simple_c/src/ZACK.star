load("@zack//c", "c_binary")

c_binary(
    out = "main",
    srcs = [
        "main.c",
    ],
)
