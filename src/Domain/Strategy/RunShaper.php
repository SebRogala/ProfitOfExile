<?php

namespace App\Domain\Strategy;

use App\Domain\Item\Fragment\UberElderShaperFragment;
use App\Domain\Item\Set\ShaperSet;

class RunShaper extends Strategy
{
    protected function setRequiredItems(): void
    {
        $this->requiredComponents = [
            [
                'item' => new ShaperSet(),
                'quantity' => 1,
            ],
        ];
    }

    public function yieldRewards(): mixed
    {
        return [
            [
                'item' => new UberElderShaperFragment(),
                'quantity' => 1,
                'probability' => 100,
            ],
        ];
    }
}
