<?php

namespace App\StrategyBuilder\Strategy;

use App\Domain\Inventory\Inventory;
use App\Domain\Trait\Name;

abstract class Strategy
{
    use Name;

    protected array $requiredComponents = [];

    protected int $averageTime = 0; //in secoconds

    protected int $occurrenceProbability = 100;

    public function __invoke(Inventory $inventory, array $data): void
    {
        $this->run($inventory, $data);
    }

    protected function run(Inventory $inventory, array $data): void
    {
        $this->averageTime = $data['averageTime'];
        $this->occurrenceProbability = $data['occurrenceProbability'];

        for ($i = 0; $i < $data['series']; $i++) {
            $this->setRequiredItems();

            foreach ($this->requiredComponents as $requiredComponent) {
                $inventory->removeItems($requiredComponent['item'], $requiredComponent['quantity']);
            }

            foreach ($this->yieldRewards() as $yieldReward) {
                $inventory->add(
                    $yieldReward['item'],
                    $yieldReward['quantity'] * ($yieldReward['probability'] / 100) * ($this->occurrenceProbability / 100)
                );
            }

            $inventory->logStrategy($this);
            $inventory->addStrategyTime($this->getAverageTime());
        }
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
}
