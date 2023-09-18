FROM scratch
COPY ./src/fixtures/hello.wasm /hello.wasm
ENTRYPOINT [ "/hello.wasm" ]
