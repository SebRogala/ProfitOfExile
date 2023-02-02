<?php

namespace App\Presentation\Strategies;

use App\Domain\Inventory\Inventory;
use App\Domain\Strategy\Grand\WitnessedShaperGuardianRotationWithHarvestEndedWithShaper;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class TheFormedWithShaper extends AbstractController
{
    #[Route("/strat/the-formed-with-shaper", name: "the-formed-with-shaper", methods: ["GET"])]
    public function index(Inventory $inventory): Response
    {
        (new WitnessedShaperGuardianRotationWithHarvestEndedWithShaper())($inventory);

        return new JsonResponse($inventory->getEndSummary());
    }
}
