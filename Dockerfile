ARG OPTIMIZER_IMAGE=cosmwasm/optimizer:0.16.1
FROM ${OPTIMIZER_IMAGE}
RUN apk add --no-cache clang


# # Update rust version
# RUN rustup update 1.88.0
# RUN rustup default 1.88.0
# RUN rustup target add wasm32-unknown-unknown
# RUN rustc --version