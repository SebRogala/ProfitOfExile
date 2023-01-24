<?php

namespace App\Domain\Strategy;

use App\Domain\Inventory\Inventory;

abstract class Strategy
{
    protected array $requiredComponents = [];

    protected array $addedStrategies = [];

    protected int $averageTime = 0;

    public function run(Inventory $inventory): void
    {
        $this->setRequiredItems();

        foreach ($this->requiredComponents as $requiredComponent) {
            $inventory->removeItems($requiredComponent['item']);
        }

        if (!empty($this->addedStrategies)) {
            /* @var $addedStrategy Strategy */
            foreach ($this->addedStrategies as $addedStrategy) {
                $addedStrategy->run($inventory);
            }
        }

        foreach ($this->yieldRewards() as $yieldReward) {
            $inventory->add($yieldReward['item']);
        }
    }

    public function combineWith(Strategy $strategy): void
    {
        $this->addedStrategies[] = $strategy;
    }

    abstract protected function yieldRewards(): mixed;

    abstract protected function setRequiredItems(): void;

//    abstract protected function setAverageTime(): void;

    protected function checkForRequiredItems(Inventory $inventory)
    {
    }
}
