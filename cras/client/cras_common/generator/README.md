1. Copy
```
cras_audio_format.h
cras_iodev_info.h
cras_messages.h
cras_types.h
```

From cras/src/common from commit
5fd5e32c111ad28da0bb860d023b281ae16c2094

to `c_headers/`

2. Modify `cras_server_state` from
`__attribute__ ((packed, aligned(4)))`
to
`__attribute__ ((packed))`

3. And use command
```
cargo run
```

to generate `gen.rs`

4. Copy `gen.rs` to
`cras_common/src/gen.rs`
