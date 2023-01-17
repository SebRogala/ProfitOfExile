<?php

namespace App\Infrastructure\Pricer;

use App\Application\Query\Pricer\PricesQuery;

class Pricer
{
    public function __construct(private PricesQuery $pricesQuery)
    {
    }


}
