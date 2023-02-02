<?php

namespace App\Presentation\Strategies;

use App\Domain\Inventory\Inventory;
use App\Domain\Item\Fragment\UberElderElderFragment;
use App\Domain\Strategy\Grand\WitnessedShaperGuardianRotationEndedWithShaper;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class HuntUberElderSets extends AbstractController
{
    #[Route("/strat/hunt-uber-elder-sets", name: "hunt-uber-elder-sets", methods: ["GET"])]
    public function index(Inventory $inventory): Response
    {
        (new WitnessedShaperGuardianRotationEndedWithShaper())($inventory);
        (new WitnessedShaperGuardianRotationEndedWithShaper())($inventory);
        $inventory->buy(new UberElderElderFragment(), 2);
//        (new WitnessedElderGuardianRotation())($inventory);
//        (new WitnessedElderGuardianRotation())($inventory);

        return new JsonResponse($inventory->getEndSummary());
    }
}
