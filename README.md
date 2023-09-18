# containerd-shim-chibiwasm
containerd-shim for [chibiwasm](https://github.com/skanehira/chibiwasm)

## build

```sh
$ apt install pkg-config libdbus-1-dev libseccomp-dev
$ cargo build --release
```

## Run

```sh
skanehira@pi1:~/work$ ls -la /usr/local/bin/
total 118208
drwxr-xr-x  2 root root     4096 Sep 18 12:46 .
drwxr-xr-x 10 root root     4096 Feb 18  2023 ..
-rwxr-xr-x  1 root root 40118648 Jun  3 08:06 containerd
-rwxr-xr-x  1 root root  6422528 Jun  3 08:06 containerd-shim
-rwxr-xr-x  1 root root 15901680 Sep 18 12:46 containerd-shim-chibiwasm-v1
-rwxr-xr-x  1 root root  8060928 Jun  3 08:06 containerd-shim-runc-v1
-rwxr-xr-x  1 root root 11534336 Jun  3 08:06 containerd-shim-runc-v2
-rwxr-xr-x  1 root root 18939904 Jun  3 08:06 containerd-stress
-rwxr-xr-x  1 root root 20054016 Jun  3 08:06 ctr
skanehira@pi1:~/work$ cat Dockerfile 
FROM scratch
COPY ./hello.wasm /hello.wasm
ENTRYPOINT [ "/hello.wasm" ]
skanehira@pi1:~/work$ sudo ctr images list
REF                                 TYPE                                    DIGEST                                                                  SIZE      PLATFORMS                   LABELS 
docker.io/library/hello-wasm:latest application/vnd.oci.image.index.v1+json sha256:fd9c651890a70a636941e5083fc4b914f677aa5f22e7fafa5abcbdeae625023e 503.0 KiB linux/arm64,unknown/unknown -      
skanehira@pi1:~/work$ sudo ctr run --rm --runtime=io.containerd.chibiwasm.v1 docker.io/library/hello-wasm:latest hello-wasm
Hello, World!
skanehira@pi1:~/work$ 
```
