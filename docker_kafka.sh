docker run --rm -ti --name=postgres-1 --rm \
  -p 5432:5432 \
  -e POSTGRES_USER=api \
  -e POSTGRES_PASSWORD=api \
  -e POSTGRES_DB=api \
  -v $(pwd)/order-watcher/test-data/database.sql:/docker-entrypoint-initdb.d/init.sql:ro \
  postgres:latest &

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
