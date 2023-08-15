<?php

namespace App\Infrastructure\Pricer\Query;

use App\Application\Command\PriceRegistry\UpdateRegistry;
use App\Application\CommandBus;
use App\Application\Query\Pricer\PricesQuery;
use App\Item\Currency\DivineOrb;
use App\Item\Item;

class FilePricerQuery implements PricesQuery
{
    private array $data = [];

    public function __construct(
        private CommandBus $commandBus,
        private string $dataDir,
        private string $priceRegistryFile
    ) {
        $this->commandBus->handle(new UpdateRegistry());

        $path = $this->dataDir.'/'.$this->priceRegistryFile;
        $jsonString = file_get_contents($path);
        $this->data = json_decode($jsonString, true);
    }

    public function findDataFor(Item $item): array
    {
        foreach ($this->data as $data) {
            if (!empty($data['item']) && $data['item'] == $item::class) {
                return $data;
            }
        }

        return [];
    }

    public function getDivinePrice(): float
    {
        return $this->findDataFor(new DivineOrb())['ninjaInChaos'];
    }
}
