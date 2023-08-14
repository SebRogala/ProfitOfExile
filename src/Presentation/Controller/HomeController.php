<?php

namespace App\Presentation\Controller;

use App\Item\ItemPrice;
use Doctrine\ODM\MongoDB\DocumentManager;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class HomeController extends AbstractController
{
    #[Route('/', name: 'app_home')]
    public function index(): Response
    {
        return $this->redirectToRoute('vue_home');
    }

    #[Route('/test', name: 'test')]
    public function test(DocumentManager $documentManager): Response
    {
        //$price = new ItemPrice();
        //$documentManager->persist($price);
        //$documentManager->flush();

dd($documentManager->getRepository(ItemPrice::class)->findAll());
        //return new JsonResponse(['ok']);
        return new JsonResponse();
    }
}
