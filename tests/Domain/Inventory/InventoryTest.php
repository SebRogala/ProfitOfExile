<?php

namespace App\Tests\Domain\Inventory;

use App\Domain\Inventory\Inventory;
use App\Item\Currency\ChaosOrb;
use App\Item\Currency\DivineOrb;
use PHPUnit\Framework\TestCase;

class InventoryTest extends TestCase
{
    public function testInventoryAddsItems()
    {
        $chaos = new ChaosOrb();
        $div = new DivineOrb();

        $inventory = new Inventory();

        $inventory->add($chaos);
        $inventory->add($div, 3);
        $inventory->add($chaos);

        $itemsInInventory = $inventory->getItems();

        self::assertNotEmpty($itemsInInventory);
        self::assertSame(2, $itemsInInventory[$chaos::class]);
        self::assertSame(3, $itemsInInventory[$div::class]);
    }
}
