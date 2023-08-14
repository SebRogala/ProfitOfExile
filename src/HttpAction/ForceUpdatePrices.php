<?php

namespace App\HttpAction;

use App\PriceUpdater\UpdatePrices;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class ForceUpdatePrices extends AbstractController
{
    #[Route("/force-update-prices", name: "force_update_prices", methods: ["GET"])]
    public function execute(UpdatePrices $updatePrices): Response
    {
        $updatePrices->handle(true);

        return new Response();
    }
}
