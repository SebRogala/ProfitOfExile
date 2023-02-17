<?php

namespace App\Infrastructure\Pricer;

use App\Application\Query\Pricer\PricesQuery;
use App\Domain\Inventory\Inventory;

class Pricer
{

    public function __construct(private PricesQuery $pricesQuery)
    {
    }

    public function priceInventory(Inventory $inventory): array
    {
        $result = [
            'items' => [],
            'bought' => [],
            'totalWorthInChaos' => 0,
        ];

        foreach ($inventory->getItems() as $itemName => $item) {
            $price = $this->getPriceForSelling($itemName);

            $result['totalWorthInChaos'] += $price * $item['quantity'];
            $result['items'][$itemName] = [
                'singularPrice' => $price,
                'quantity' => $item['quantity'],
                'summedPrice' => $price * $item['quantity'],
            ];
        }

        $boughtSummary = $inventory->getBuyerSummary();

        $result['bought'] = $boughtSummary;
        $result['totalExpenses'] = 0;

        if (!empty($boughtSummary)) {
            foreach ($boughtSummary as $item) {
                $result['totalExpenses'] += $item['totalPrice'];
            }
        }

        $result['profit'] = $result['totalWorthInChaos'] - $result['totalExpenses'];

        $result['chaosPerHour'] = $this->calculatePricePerHour($inventory->getTotalRunTime(),  $result['profit']);
        $result['divPerHour'] = $result['chaosPerHour'] / $this->pricesQuery->getDivinePrice();

        return $result;
    }

    public function priceStrategies(array $evaluatedStrategies): array
    {
        $result = [];
        $totalTime = 0;

        foreach ($evaluatedStrategies as $name => $strategy) {
            $rewards = $this->evaluateStrategyRewards($strategy);
            $result[$name] = [
                'time' => $strategy['time'],
                'expenses' => $this->evaluateStrategyExpenses($strategy['expenses']),
            ];

            $result[$name] = array_merge_recursive($result[$name], $rewards);
            $totalTime += $strategy['time'];
        }

        return [
            'totalTime' => $totalTime,
            'strategies' => $result,
        ];
    }

    private function evaluateStrategyExpenses(array $expenses): array
    {
        if (empty($expenses)) {
            return [];
        }

        $result = [];

        foreach ($expenses as $name => $expense) {
            $price = $this->getPriceForSelling($name);
            $result[$name] = [
                'singularPrice' => $price,
                'quantity' => $expense['quantity'],
                'summedPrice' => $price * $expense['quantity'],
            ];
        }

        return $result;
    }

    private function evaluateStrategyRewards(array $strategy): array
    {
        if (empty($strategy['rewards'])) {
            return [];
        }

        $result = [
            'rewards' => [],
            'summedPrice' => 0,
            'chaosPerHour' => 0,
            'divPerHour' => 0,
        ];

        foreach ($strategy['rewards'] as $name => $reward) {
            $price = $this->getPriceForSelling($name);
            $summedPrice = $price * $reward['quantity'] * ($reward['probability'] / 100);
            $result['rewards'][$name] = [
                'probability' => $reward['probability'],
                'singularPrice' => $price,
                'quantity' => $reward['quantity'],
                'summedPrice' => $summedPrice,
                'chaosPerHour' => $this->calculatePricePerHour($strategy['time'], $summedPrice),
            ];

            $result['summedPrice'] += $summedPrice;
        }

        $result['chaosPerHour'] = $this->calculatePricePerHour($strategy['time'],  $result['summedPrice']);
        $result['divPerHour'] = $result['chaosPerHour'] / $this->pricesQuery->getDivinePrice();

        return $result;
    }

    private function calculatePricePerHour(int $time, float $price): float
    {
        if ($time === 0) {
            return 0;
        }

        return $price * (3600 / $time);
    }

    private function calculateSummary($result, $boughtSummary)
    {
        $result['bought'] = $boughtSummary;
        $result['totalExpenses'] = 0;

        foreach ($boughtSummary as $item) {
            $result['totalExpenses'] += $item['totalPrice'];
        }

        $result['profit'] = $result['totalWorthInChaos'] - $result['totalExpenses'];

        return $result;
    }

    private function getPriceForSelling($item): float
    {
        $itemPriceData = $this->pricesQuery->findDataFor($item);

        if (isset($itemPriceData['tftInChaos'])) {
            $price = $itemPriceData['tftInChaos'];
        } else {
            $price = $itemPriceData['ninjaInChaos'];
        }

        return $price;
    }
}
