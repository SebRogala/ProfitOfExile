<?php

declare(strict_types=1);

namespace App\Item;

use Doctrine\ODM\MongoDB\Mapping\Annotations as ODM;
use Doctrine\ODM\MongoDB\Types\Type;

#[ODM\Document]
class ItemPrice
{
    #[ODM\Id]
    private $id;

    #[ODM\Field(type: Type::STRING)]
    private string $name = 'test';
}
