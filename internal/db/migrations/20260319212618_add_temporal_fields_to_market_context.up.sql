ALTER TABLE market_context ADD COLUMN hourly_volatility JSONB NOT NULL DEFAULT '[]';
ALTER TABLE market_context ADD COLUMN hourly_activity JSONB NOT NULL DEFAULT '[]';
ALTER TABLE market_context ADD COLUMN weekday_volatility JSONB NOT NULL DEFAULT '[]';
ALTER TABLE market_context ADD COLUMN weekday_activity JSONB NOT NULL DEFAULT '[]';
