DO $$
BEGIN
    RAISE EXCEPTION '20260723160017_league_scope_gem_snapshots is forward-only; restore from a pre-migration backup';
END
$$;
