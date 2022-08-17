# 0x Order Watcher

Watches 0x orders for activity and changes.

## Crate features

* `proptest` Implement [`proptest::arbitrary::Arbitrary`](https://docs.rs/proptest/1.0.0/proptest/arbitrary/trait.Arbitrary.html) for some types, required for benchmarking.
* `bench` This feature is only used for benchmarking and exposes a function that runs
  criterion tests.

## Building and testing

### Dependencies

The Diesel ORM requires `libpq` for PostgreSQL support.

```shell
brew install libpq
brew link libpq --force
```

### Services

#### Kafka

```shell
docker run --rm -ti  --name=redpanda-1 --rm \
  -p 9092:9092 \
  docker.vectorized.io/vectorized/redpanda:latest \
  start \
  --overprovisioned \
  --smp 1  \
  --memory 1G \
  --reserve-memory 0M \
  --node-id 0 \
  --check=false
```

#### PostgreSQL

```shell
docker run --rm -ti --name=postgres-1 --rm \
  -p 5432:5432 \
  -e POSTGRES_USER=api \
  -e POSTGRES_PASSWORD=api \
  -e POSTGRES_DB=api \
  -v $(pwd)/order-watcher/test-data/database.sql:/docker-entrypoint-initdb.d/init.sql:ro \
  postgres:latest
```

#### Order watcher

Directly

```shell
cargo run -- -vv
```

Docker image

```shell
aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin 883408475785.dkr.ecr.us-east-1.amazonaws.com && \
docker run --rm -ti \
  --pull always \
  --publish 8080:8080 \
  --publish 9998:9998 \
  883408475785.dkr.ecr.us-east-1.amazonaws.com/0x/order-watcher:17563a3563330677dd2f7086c9f0c3f5716c2ea5 \
  --kafka host.docker.internal:9092 \
  --database postgres://api:api@host.docker.internal:5432/api \
  --log-format json -vv
```

Post 5 orders from SRA

```shell
curl "https://api.0x.org/sra/v4/orders?perPage=5" | jq "[.records[].order]" | curl -H "Content-Type: application/json" -X POST -d @- "https://demesh.staging.api.0x.org/sra/v4/orders"
```

View metrics

```shell
curl "http://127.0.0.1:9998/metrics"
```

#### Manually building and pushing images

```shell
ECR="883408475785.dkr.ecr.us-east-1.amazonaws.com"
REPO="0x/order-watcher"
COMMIT=$()
docker build --progress=plain -t $ECR/$REPO:$COMMIT -t $ECR/$REPO:latest

aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin $ECR
docker push $ECR/$REPO:a4382f6 $ECR/$REPO:latest
```

## To do

* Maybe optimize for many invalid orders, such as only revalidating them when a re-org actually happened.
* Fix excessive allocs (suspect app.clone() line)
* Handle expiration without fetch
* Meter inserted order count and deleted order count
* Make MAX_REORG and other constants configurable in block_watcher
