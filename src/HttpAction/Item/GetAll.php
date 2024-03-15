<?php

namespace App\HttpAction\Item;

use App\Item\ItemPrice\ItemPrice;
use App\Item\ItemPrice\ItemPriceRepository;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class GetAll extends AbstractController
{
    public function __construct(private ItemPriceRepository $itemPriceRepository)
    {
    }

    #[Route("/item/get-all", name: "get-items", methods: ["GET"])]
    public function index(): Response
    {
        $ret = [];

        /** @var ItemPrice $item */
        foreach ($this->itemPriceRepository->findAll() as $item) {
            $ret[] = [
                'itemKey' => $item->nameKey,
                'itemName' => $item->name,
                'icon' => $item->iconUrl
            ];
        }

        return new JsonResponse($ret);
    }
}
