create unlogged table large_notifications (
  id serial  primary key,
  payload    varchar not null,
  created_at timestamp default current_timestamp not null
);

comment on table large_notifications is
'Table for notifications whose payload is too big to send directly';
