<?php

namespace App\Infrastructure\Pricer\Query;

use App\Application\Command\PriceRegistry\UpdateRegistry;
use App\Application\CommandBus;
use App\Application\Query\Pricer\PricesQuery;
use App\Domain\Item\Currency\DivineOrb;

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

    public function findDataFor(string $name): array
    {
        foreach ($this->data as $data) {
            if (!empty($data['item']) && $data['item'] == $name) {
                return $data;
            }
        }

        return [];
    }

    public function getDivinePrice(): float
    {
        return $this->findDataFor(DivineOrb::class)['ninjaInChaos'];
    }
}
