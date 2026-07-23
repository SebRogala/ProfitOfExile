DO $$
BEGIN
    RAISE EXCEPTION '20260723160016_create_league_control_tables is forward-only; restore from a pre-migration backup';
END
$$;
