<?php

namespace App\StrategyBuilder\Strategy;

class Wrapper extends Strategy
{
    protected function setRequiredItems(): void
    {
    }

    public function yieldRewards(): mixed
    {
        return [];
    }
}
