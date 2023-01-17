<?php

namespace App\Application\Query\Pricer;

interface PricesQuery
{
    public function findDataFor(string $name): array;
}
