<?php

namespace App\Infrastructure\Strategy;

use App\Domain\Inventory\Inventory;

class Runner
{
    public function __construct(private Factory $factory)
    {
    }

    public function handle(Inventory $inventory, array $strategies):void
    {
        foreach ($strategies as $strategyName => $data) {
//            $additionalStrategies = key_exists('strategies', $data) ? $data['strategies'] : [];

            if (key_exists('strategies', $data)) {
                for ($i = 0; $i < $data['times']; $i++) {
                    $this->handle($inventory, $data['strategies']);
                }
            }

            $strategy = $this->factory->create($strategyName);
            $strategy($inventory, $data['times']);
//            $strategy($inventory, $data['times'], $additionalStrategies);
        }
    }
}
