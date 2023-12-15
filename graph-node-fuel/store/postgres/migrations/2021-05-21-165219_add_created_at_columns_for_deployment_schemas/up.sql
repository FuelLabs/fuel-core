alter table public.deployment_schemas
    add column created_at timestamptz not null default now();

alter table public.unused_deployments
    add column created_at timestamptz;

-- use a predefined default date that doesn't have a meaning.
update
    public.unused_deployments
set
    created_at = '2000-01-01';

alter table public.unused_deployments
    alter column created_at set not null;
