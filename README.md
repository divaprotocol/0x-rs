# 0x-rs

[![Build](https://github.com/0xProject/order-watcher/actions/workflows/build.yml/badge.svg)](https://github.com/0xProject/order-watcher/actions/workflows/build.yml)
[![Checks](https://github.com/0xProject/order-watcher/actions/workflows/checks.yml/badge.svg)](https://github.com/0xProject/order-watcher/actions/workflows/checks.yml)
[![Tests](https://github.com/0xProject/order-watcher/actions/workflows/tests.yml/badge.svg)](https://github.com/0xProject/order-watcher/actions/workflows/tests.yml)
[![Coverage](https://codecov.io/gh/0xProject/order-watcher/branch/main/graph/badge.svg?token=LBSxLWTQCJ)](https://codecov.io/gh/0xProject/order-watcher)
[![Benchmark](https://github.com/0xProject/order-watcher/actions/workflows/bench.yml/badge.svg)](https://github.com/0xProject/order-watcher/actions/workflows/bench.yml)
[![Drone Status](https://drone.spaceship.0x.org/api/badges/0xProject/order-watcher/status.svg)](https://drone.spaceship.0x.org/0xProject/order-watcher)


## Pre-requirements

-   Running [Kafka Service](https://hevodata.com/blog/how-to-install-kafka-on-ubuntu/)

## Developing

1. Clone the repo.

2. Install `Cargo`. If you have already installed `Cargo`, you can skip this step.
    ```
    sudo apt install cargo
    ```

3. Install dependency modules. If you have already installed these modules you can skip this step.
    ```
    sudo apt install pkg-config -y && sudo apt install libssl-dev -y && sudo apt install cmake -y && sudo apt install libpq-dev -y
    ```
4. Running services
    - Copy the `.env` file
    ```
    cp .env.example .env
    ```
    - Block watcher service
    ```
    cd block-watcher && cargo run
    ```
    - Order watcher
    ```
    cd order-watcher && cargo run
    ```