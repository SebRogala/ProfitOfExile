<?php

namespace App\Infrastructure\Market;

use App\Application\Query\Pricer\PricesQuery;
use App\Domain\Item\Item;

class Buyer
{
    private array $summary = [];

    public function __construct(private readonly PricesQuery $pricesQuery)
    {
    }

    public function buy(Item $item, int $quantity = 1): ?Bought
    {
        $res = $this->pricesQuery->findDataFor($item);

        if (empty($res)) {
            //potentially throw new exception
            return null;
        }

        if (key_exists('ninjaInChaos', $res)) {
            $this->addToSummary($item, $quantity, $res['ninjaInChaos'] * $quantity, 'poe-ninja');
        } else {
            $this->addToSummary($item, $quantity, $res['tftInChaos'] * $quantity, 'tft');
        }

        return new Bought($item, $quantity);
    }

    private function addToSummary(Item $item, int $quantity, float $price, string $source): void
    {
        if (!key_exists($item->name(), $this->summary)) {
            $this->summary[$item->name()] = [
                'item' => $item->name(),
                'quantity' => 0,
                'totalPrice' => 0,
                'source' => '',
            ];
        }

        $this->summary[$item->name()]['quantity'] += $quantity;
        $this->summary[$item->name()]['totalPrice'] += $price;
        $this->summary[$item->name()]['source'] = $source;
    }

    public function getSummary(): array
    {
        return $this->summary;
    }
}
