<?php

namespace App\StrategyBuilder;

use App\Domain\Inventory\Inventory;

class Runner
{
    public function __construct(private Factory $factory)
    {
    }

    public function handle(Inventory $inventory, array $strategies): void
    {
        foreach ($strategies as $data) {
            if (key_exists('strategies', $data)) {
                for ($i = 0; $i < $data['series']; $i++) {
                    $this->handle($inventory, $data['strategies']);
                }
            }
            $strategy = $this->factory->create($data['key']);

            $defaults = [
                'series' => 1,
                'averageTime' => $strategy->getAverageTime(),
                'occurrenceProbability' => $strategy->getOccurrenceProbability(),
                'requiredItems' => [],
                'rewards' => [],
            ];

            $data = array_merge($defaults, $data);

            $strategy($inventory, $data);
        }
    }
}
