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

        foreach ($inventory->getItems() as $item => $quantity) {
            $itemPriceData = $this->pricesQuery->findDataFor($item);

            if (isset($itemPriceData['tftInChaos'])) {
                $price = $itemPriceData['tftInChaos'];
            } else {
                $price = $itemPriceData['ninjaInChaos'];
            }

            $result['totalWorthInChaos'] += $price * $quantity;
            $result['items'][$item] = [
                'singularPrice' => $price,
                'quantity' => $quantity,
                'summedPrice' => $price * $quantity,
            ];
        }

        $boughtSummary = $inventory->getBuyerSummary();

        if (!empty($boughtSummary)) {
            $result = $this->calculateSummary($result, $boughtSummary);
        }

        return $result;
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
}
