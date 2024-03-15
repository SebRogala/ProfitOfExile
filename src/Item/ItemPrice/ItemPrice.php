<?php

declare(strict_types=1);

namespace App\Item\ItemPrice;

use App\Item\Item;
use Doctrine\ODM\MongoDB\Mapping\Annotations as ODM;
use Doctrine\ODM\MongoDB\Types\Type;

#[ODM\Document(repositoryClass: ItemPriceRepository::class)]
class ItemPrice
{
    #[ODM\Id]
    private $id;

    #[ODM\Field(type: Type::STRING)]
    public readonly string $namespace;

    #[ODM\Field(type: Type::STRING)]
    public readonly string $name;

    #[ODM\Field(type: Type::STRING)]
    public readonly string $nameKey;

    #[ODM\Field(type: Type::STRING)]
    public readonly string $iconUrl;

    #[ODM\Field(type: Type::FLOAT)]
    public readonly ?float $ninjaInChaos;

    #[ODM\Field(type: Type::FLOAT)]
    public readonly ?float $tftInChaos;

    public function __construct(Item $item, ?float $ninjaInChaos, ?float $tftInChaos, string $iconUrl)
    {
        $this->namespace = $item::class;
        $this->name = $item->name();
        $this->nameKey = $item->nameKey();
        $this->ninjaInChaos = $ninjaInChaos;
        $this->tftInChaos = $tftInChaos;
        $this->iconUrl = $iconUrl;
    }
}
