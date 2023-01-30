<?php

namespace App\Domain\Strategy;

use App\Domain\Inventory\Inventory;

abstract class Strategy
{
    protected array $requiredComponents = [];

    protected array $addedStrategies = [];

    protected int $averageTime = 0;

    protected int $occurrenceProbability = 100;

    public function __invoke(Inventory $inventory, int $cycles = 1, $strategies = []): void
    {
        if (!empty($strategies)){
            if (!is_array($strategies)) {
                $strategies = [$strategies];
            }

            foreach ($strategies as $strategy) {
                $this->combineWith($strategy);
            }
        }

        $this->run($inventory, $cycles);
    }

    public function run(Inventory $inventory, int $cycles = 1): void
    {
        for ($i = 0; $i < $cycles; $i++) {
            $this->setRequiredItems();

            foreach ($this->requiredComponents as $requiredComponent) {
                $inventory->removeItems($requiredComponent['item'], $requiredComponent['quantity']);
            }

            if (!empty($this->addedStrategies)) {
                /* @var $addedStrategy Strategy */
                foreach ($this->addedStrategies as $addedStrategy) {
                    $addedStrategy->run($inventory);
                }
            }

            foreach ($this->yieldRewards() as $yieldReward) {
                $inventory->add($yieldReward['item'], $yieldReward['quantity']);
            }

            $inventory->logStrategy($this);
        }
    }

    public function combineWith(Strategy $strategy): void
    {
        $this->addedStrategies[] = $strategy;
    }

    public function getAverageTime(): int
    {
        return $this->averageTime;
    }

    public function getRequiredItems(): array
    {
        return $this->requiredComponents;
    }

    public function getOccurrenceProbability(): int
    {
        return $this->occurrenceProbability;
    }

    abstract protected function setRequiredItems(): void;

    abstract public function yieldRewards(): mixed;

//    abstract protected function setAverageTime(): void;
}
