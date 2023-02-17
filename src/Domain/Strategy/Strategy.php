<?php

namespace App\Domain\Strategy;

use App\Domain\Inventory\Inventory;

abstract class Strategy
{
    protected array $requiredComponents = [];

    protected int $averageTime = 0;

    protected int $occurrenceProbability = 100;

    public function __invoke(Inventory $inventory, array $data): void
    {
        $this->run($inventory, $data);
    }

    public function run(Inventory $inventory, array $data): void
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

    public function name(): string
    {
        $splitNamespace = explode('\\', static::class);

        return array_pop($splitNamespace);
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
