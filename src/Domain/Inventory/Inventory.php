<?php

namespace App\Domain\Inventory;

use App\Domain\Item\Item;
use App\Domain\Strategy\Strategy;
use App\Infrastructure\Market\Buyer;
use App\Infrastructure\Pricer\Pricer;

class Inventory
{
    private array $evaluatedStrategies = [];

    private int $totalRunTime = 0;

    private array $items = [];

    public function __construct(private SetConverter $setConverter, private Buyer $buyer, private Pricer $pricer)
    {
    }

    public function add(Item $item, float $quantity = 1): void
    {
        if (!$this->hasItems($item, 0)) {
            $this->items[$item::class] = [
                "item" => $item,
                "quantity" => 0,
            ];
        }
        $this->items[$item::class]["quantity"] += $quantity;

        $this->setConverter->convertToSets($this);
    }

    public function getItems(): array
    {
        return $this->items;
    }

    public function hasItems(Item $item, $quantity = 1): bool
    {
        if (empty($this->items[$item::class])) {
            return false;
        }

        if ($this->items[$item::class]["quantity"] < $quantity) {
            return false;
        }

        return true;
    }

    public function buy(Item $item, int $quantity = 1): void
    {
        $boughtItems = $this->buyer->buy($item, $quantity);
        $this->add($boughtItems->item(), $boughtItems->quantity());
    }

    public function removeItems(Item $item, int $quantity = 1): void
    {
        if (!$this->hasItems($item, $quantity)) {
            $this->buy($item, $quantity);
        }

        $this->items[$item::class]["quantity"] -= $quantity;

        if ($this->items[$item::class]["quantity"] <= 0) {
            unset($this->items[$item::class]);
        }
    }

    public function getBuyerSummary(): array
    {
        return $this->buyer->getSummary();
    }

    public function evaluateItems(): array
    {
        return $this->pricer->priceInventory($this);
    }

    public function getEndSummary(): array
    {
        return array_merge_recursive(
            [
                'totalTimeInMinutes' => $this->getTotalRunTime() / 60,
            ],
            $this->evaluateStrategies(),
            $this->evaluateItems()
        );
    }

    public function evaluateStrategies(): array
    {
        return $this->pricer->priceStrategies($this->evaluatedStrategies);
    }

    public function addStrategyTime(int $seconds): void
    {
        $this->totalRunTime += $seconds;
    }

    public function getTotalRunTime(): int
    {
        return $this->totalRunTime;
    }

    public function logStrategy(Strategy $strategy): void
    {
        if (!array_key_exists($strategy::class, $this->evaluatedStrategies)) {
            $this->evaluatedStrategies[$strategy::class] = [
                'ranTimes' => 0,
                'time' => 0,
                'occurrenceProbability' => $strategy->getOccurrenceProbability(),
                'expenses' => [],
                'rewards' => [],
            ];
        }

        $this->evaluatedStrategies[$strategy::class]['ranTimes']++;
        $this->evaluatedStrategies[$strategy::class]['time'] += $strategy->getAverageTime();

        foreach ($strategy->getRequiredItems() as $requiredComponent) {
            if (!array_key_exists(
                $requiredComponent['item']::class,
                $this->evaluatedStrategies[$strategy::class]['expenses']
            )) {
                $this->evaluatedStrategies[$strategy::class]['expenses'][$requiredComponent['item']::class] = [
                    'item' => $requiredComponent['item'],
                    'quantity' => 0,
                ];
            }

            $this->evaluatedStrategies[$strategy::class]['expenses'][$requiredComponent['item']::class]['quantity'] += $requiredComponent['quantity'];
        }

        foreach ($strategy->yieldRewards() as $reward) {
            if (!array_key_exists(
                $reward['item']::class,
                $this->evaluatedStrategies[$strategy::class]['rewards']
            )) {
                $this->evaluatedStrategies[$strategy::class]['rewards'][$reward['item']::class] = [
                    'probability' => $reward['probability'],
                    'item' => $reward['item'],
                    'quantity' => 0,
                ];
            }

            $this->evaluatedStrategies[$strategy::class]['rewards'][$reward['item']::class]['quantity'] += $reward['quantity'];
        }
    }
}
