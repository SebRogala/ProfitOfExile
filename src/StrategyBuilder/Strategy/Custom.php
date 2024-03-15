<?php

namespace App\StrategyBuilder\Strategy;

use App\Domain\Inventory\Inventory;
use App\Item\Factory;

class Custom extends Strategy
{
    // name (optional)
    // description (optional)

    private array $rewardItems = [];

    public function __construct(private Factory $itemFactory)
    {
    }

    public function __invoke(Inventory $inventory, array $data): void
    {
        foreach ($data['requiredItems'] as $required) {
            $this->requiredComponents[] = [
                'item' => $this->itemFactory->create($required['item']),
                'quantity' => $required['quantity'],
            ];
        }

        foreach ($data['rewards'] as $reward) {
            $this->rewardItems[] =
                [
                    'item' => $this->itemFactory->create($reward['item']),
                    'quantity' => $reward['quantity'],
                    'probability' => $reward['probability'],
                ];
        }

        $this->run($inventory, $data);
    }

    protected function setRequiredItems(): void
    {
    }

    public function yieldRewards(): mixed
    {
        return $this->rewardItems;
    }
}
