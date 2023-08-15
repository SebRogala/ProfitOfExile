<?php

declare(strict_types=1);

namespace App\Item\ItemPrice;

use App\Item\Item;
use Doctrine\ODM\MongoDB\DocumentManager;
use Doctrine\ODM\MongoDB\MongoDBException;
use Doctrine\ODM\MongoDB\Repository\DocumentRepository;

class ItemPriceRepository extends DocumentRepository
{
    public function __construct(DocumentManager $dm)
    {
        $uow = $dm->getUnitOfWork();
        $classMetaData = $dm->getClassMetadata(ItemPrice::class);
        parent::__construct($dm, $uow, $classMetaData);
    }

    public function get(Item $item): ItemPrice
    {
        return $this->findOneBy(['name' => $item::class]);
    }

    public function removeAll(): void
    {
        $this
            ->dm
            ->createQueryBuilder(ItemPrice::class)
            ->remove()
            ->getQuery()
            ->execute();
    }

    /**
     * @param ItemPrice[] $items
     * @return void
     * @throws MongoDBException
     */
    public function addMany(array $items): void
    {
        foreach ($items as $item) {
            $this->dm->persist($item);
        }

        $this->dm->flush();
    }
}
