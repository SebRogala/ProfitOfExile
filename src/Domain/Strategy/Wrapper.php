<?php

namespace App\Domain\Strategy;

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
