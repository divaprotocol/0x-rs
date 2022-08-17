docker run --rm -ti --name=postgres-1 --rm \
  -p 5432:5432 \
  -e POSTGRES_USER=api \
  -e POSTGRES_PASSWORD=api \
  -e POSTGRES_DB=api \
  -v $(pwd)/order-watcher/test-data/database.sql:/docker-entrypoint-initdb.d/init.sql:ro \
  postgres:latest
