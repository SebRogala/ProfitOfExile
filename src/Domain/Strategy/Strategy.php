<?php

namespace App\Domain\Strategy;

use App\Domain\Inventory\Inventory;

abstract class Strategy
{
    protected array $requiredComponents = [];

    protected int $averageTime = 0;


    abstract public function yieldRewards(): mixed;

    abstract protected function setRequiredItems(): void;

    abstract protected function setAverageTime(): void;

    protected function checkForRequiredItems(Inventory $inventory)
    {

    }
}
