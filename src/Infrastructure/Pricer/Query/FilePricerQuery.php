<?php

namespace App\Infrastructure\Pricer\Query;

use App\Application\Query\Pricer\PricesQuery;

class FilePricerQuery implements PricesQuery
{
    private array $data = [];

    public function __construct(
        private string $dataDir,
        private string $priceRegistryFile
    ) {
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
}
