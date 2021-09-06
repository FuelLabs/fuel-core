# Install the diesel-cli tool

```cargo install diesel_cli --no-default-features --features postgres```

# Setting up

```
docker run --rm -p 5432:5432 --name postgres-db -e POSTGRES_PASSWORD=my-secret -d postgres
echo 'DATABASE_URL=postgres://postgres:my-secret@127.0.0.1:5432' >> .env
diesel migration run 
```

# Upgrading the schema

```
diesel migration generate <schema_update_name>
```

edit migration schema, which will be in `migrations/<date>_<version>_<schema_update_name>/up.sql` and `down.sql`


Then run the migration:
```
disel migration run 
```
